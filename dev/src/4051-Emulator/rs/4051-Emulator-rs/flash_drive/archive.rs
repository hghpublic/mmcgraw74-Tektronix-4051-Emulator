//! Whole-storage Flash Drive ZIP interchange, independent of tape state.
//!
//! Export streams borrowed files in caller order as JSON-header/content pairs.
//! Import validates every pair under shared compressed/expanded limits before
//! returning numbered files; entry names are ignored and the last duplicate wins.
//! Callers own filesystem operations and committing the decoded store.

use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::{ArchiveHeader, FileType, FileUsage, encode_archive_header, parse_archive_header};

#[cfg(test)]
mod tests;

/// Maximum total ZIP entries accepted in one Flash Drive import operation.
pub const MAX_FLASH_ARCHIVE_ENTRIES: usize = 4_096;
/// Maximum expanded size of one Flash Drive ZIP entry.
pub const MAX_FLASH_ARCHIVE_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum total expanded bytes accepted in one Flash Drive import operation.
pub const MAX_FLASH_ARCHIVE_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum compressed size accepted for one Flash Drive ZIP file.
pub const MAX_FLASH_ARCHIVE_FILE_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) struct ArchiveFileRef<'a> {
    pub(crate) number: u8,
    pub(crate) file_type: Option<FileType>,
    pub(crate) usage: Option<FileUsage>,
    pub(crate) description: &'a [u16],
    pub(crate) bytes: &'a [u8],
}

#[cfg(feature = "wasm")]
pub(crate) struct OwnedArchiveFile {
    pub(crate) number: u8,
    pub(crate) file_type: Option<FileType>,
    pub(crate) usage: Option<FileUsage>,
    pub(crate) description: Vec<u16>,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct ArchiveFile {
    pub(crate) header: ArchiveHeader,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn write_archive<'a, W: Write + Seek>(
    destination: W,
    files: impl ExactSizeIterator<Item = ArchiveFileRef<'a>>,
) -> Result<W, String> {
    if files.len() == 0 {
        return Err("Flash Drive contains no files".to_owned());
    }

    let mut archive = ZipWriter::new(destination);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (index, file) in files.enumerate() {
        let header =
            encode_archive_header(file.number, file.file_type, file.usage, file.description)?;
        let header_name = format!("{:04}", index * 2 + 1);
        archive
            .start_file(&header_name, options)
            .map_err(|error| format!("cannot create ZIP entry {header_name}: {error}"))?;
        archive
            .write_all(header.as_bytes())
            .map_err(|error| format!("cannot write ZIP entry {header_name}: {error}"))?;

        let content_name = format!("{:04}", index * 2 + 2);
        archive
            .start_file(&content_name, options)
            .map_err(|error| format!("cannot create ZIP entry {content_name}: {error}"))?;
        archive
            .write_all(file.bytes)
            .map_err(|error| format!("cannot write ZIP entry {content_name}: {error}"))?;
    }
    archive
        .finish()
        .map_err(|error| format!("cannot finish Flash Drive archive: {error}"))
}

pub(crate) fn read_archive<R: Read + Seek>(
    mut source: R,
) -> Result<BTreeMap<u8, ArchiveFile>, String> {
    let compressed_size = source
        .seek(SeekFrom::End(0))
        .map_err(|error| format!("cannot determine ZIP size: {error}"))?;
    if compressed_size > MAX_FLASH_ARCHIVE_FILE_BYTES {
        return Err(format!(
            "ZIP exceeds the {} MiB compressed file limit",
            MAX_FLASH_ARCHIVE_FILE_BYTES / (1024 * 1024)
        ));
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("cannot rewind ZIP archive: {error}"))?;
    let mut archive =
        ZipArchive::new(source).map_err(|error| format!("invalid ZIP archive: {error}"))?;
    if archive.len() > MAX_FLASH_ARCHIVE_ENTRIES {
        return Err(format!(
            "ZIP archive contains {} entries, exceeding the {} entry limit",
            archive.len(),
            MAX_FLASH_ARCHIVE_ENTRIES
        ));
    }
    if archive.len() % 2 != 0 {
        return Err("whole-storage ZIP archive must contain header/content entry pairs".to_owned());
    }

    let mut remaining_bytes = MAX_FLASH_ARCHIVE_TOTAL_BYTES;
    let mut files = BTreeMap::new();
    for pair in 0..archive.len() / 2 {
        let header_index = pair * 2;
        let header_entry = archive
            .by_index(header_index)
            .map_err(|error| format!("cannot open ZIP entry {header_index}: {error}"))?;
        if header_entry.is_dir() {
            return Err(format!(
                "ZIP entry {header_index} is a directory, not a header"
            ));
        }
        let header_name = header_entry.name().to_owned();
        let header_bytes = read_entry(header_entry, &header_name, &mut remaining_bytes)?;
        let header = parse_archive_header(&header_bytes)?;
        let content_index = header_index + 1;
        let content_entry = archive
            .by_index(content_index)
            .map_err(|error| format!("cannot open ZIP entry {content_index}: {error}"))?;
        if content_entry.is_dir() {
            return Err(format!(
                "ZIP entry {content_index} is a directory, not file contents"
            ));
        }
        let content_name = content_entry.name().to_owned();
        let bytes = read_entry(content_entry, &content_name, &mut remaining_bytes)?;
        files.insert(header.number, ArchiveFile { header, bytes });
    }
    Ok(files)
}

fn read_entry<R: Read>(entry: R, name: &str, remaining_bytes: &mut u64) -> Result<Vec<u8>, String> {
    let entry_limit = MAX_FLASH_ARCHIVE_ENTRY_BYTES.min(*remaining_bytes);
    let mut bytes = Vec::new();
    let expanded_bytes = entry
        .take(entry_limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read ZIP entry {name}: {error}"))?
        as u64;
    if expanded_bytes > MAX_FLASH_ARCHIVE_ENTRY_BYTES {
        return Err(format!(
            "ZIP entry {name} exceeds the {} MiB uncompressed entry limit",
            MAX_FLASH_ARCHIVE_ENTRY_BYTES / (1024 * 1024)
        ));
    }
    if expanded_bytes > *remaining_bytes {
        return Err(format!(
            "ZIP archive exceeds the {} MiB total uncompressed limit at entry {name}",
            MAX_FLASH_ARCHIVE_TOTAL_BYTES / (1024 * 1024)
        ));
    }
    *remaining_bytes -= expanded_bytes;
    Ok(bytes)
}
