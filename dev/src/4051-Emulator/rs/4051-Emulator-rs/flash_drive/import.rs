//! Shared bounded decoder for ZIP files selected for Flash Drive import.
//!
//! The decoder preserves archive order and reports damaged members independently.
//! A selection-level budget shares entry and expanded-byte limits across ZIPs;
//! host adapters remain responsible for reading files and supplying the names.

use std::io::{Cursor, Read, Seek};

use crate::{
    flash_drive::archive::{
        MAX_FLASH_ARCHIVE_ENTRIES, MAX_FLASH_ARCHIVE_ENTRY_BYTES, MAX_FLASH_ARCHIVE_FILE_BYTES,
        MAX_FLASH_ARCHIVE_TOTAL_BYTES,
    },
    machine::gpib::FlashDriveFile,
};

/// Selected loose files and ZIP members, per-input errors, and their original order.
#[derive(Default)]
pub struct FlashImportSelection {
    pub files: Vec<FlashDriveFile>,
    pub errors: Vec<String>,
    /// Indices that reconstruct success/error order across selected files and ZIP members.
    pub order: Vec<FlashImportOrder>,
}

/// An index into one of the selected-file batch's parallel result collections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashImportOrder {
    File(usize),
    Error(usize),
}

impl FlashImportSelection {
    /// Adds one valid file at the end of the selection order.
    pub fn push_file(&mut self, file: FlashDriveFile) {
        let index = self.files.len();
        self.files.push(file);
        self.order.push(FlashImportOrder::File(index));
    }

    /// Adds one failed input at the end of the selection order.
    pub fn push_error(&mut self, error: String) {
        let index = self.errors.len();
        self.errors.push(error);
        self.order.push(FlashImportOrder::Error(index));
    }

    /// Appends a selection and adjusts its indices to preserve the combined order.
    pub fn append(&mut self, mut other: Self) {
        let file_offset = self.files.len();
        let error_offset = self.errors.len();
        self.order
            .extend(other.order.drain(..).map(|item| match item {
                FlashImportOrder::File(index) => FlashImportOrder::File(file_offset + index),
                FlashImportOrder::Error(index) => FlashImportOrder::Error(error_offset + index),
            }));
        self.files.append(&mut other.files);
        self.errors.append(&mut other.errors);
    }

    /// Validate that each success and failure appears exactly once in the order index.
    pub fn validate_order(&self) -> Result<(), String> {
        let mut seen_files = vec![false; self.files.len()];
        let mut seen_errors = vec![false; self.errors.len()];
        for item in &self.order {
            let seen = match *item {
                FlashImportOrder::File(index) => seen_files.get_mut(index),
                FlashImportOrder::Error(index) => seen_errors.get_mut(index),
            }
            .ok_or_else(|| "Flash Drive import order contains an invalid index".to_owned())?;
            if std::mem::replace(seen, true) {
                return Err("Flash Drive import order contains a duplicate index".to_owned());
            }
        }
        if seen_files.iter().any(|seen| !seen) || seen_errors.iter().any(|seen| !seen) {
            return Err("Flash Drive import order omits a selected result".to_owned());
        }
        Ok(())
    }
}

/// Remaining ZIP entry and expanded-byte allowance for one selected-file batch.
pub struct FlashArchiveBudget {
    pub remaining_entries: usize,
    pub remaining_bytes: u64,
}

impl Default for FlashArchiveBudget {
    fn default() -> Self {
        Self {
            remaining_entries: MAX_FLASH_ARCHIVE_ENTRIES,
            remaining_bytes: MAX_FLASH_ARCHIVE_TOTAL_BYTES,
        }
    }
}

/// Decode bytes from a selected ZIP while enforcing the compressed-input limit.
pub fn read_flash_archive_bytes(
    bytes: &[u8],
    budget: &mut FlashArchiveBudget,
) -> Result<FlashImportSelection, String> {
    if bytes.len() as u64 > MAX_FLASH_ARCHIVE_FILE_BYTES {
        return Err(format!(
            "ZIP exceeds the {} MiB compressed file limit",
            MAX_FLASH_ARCHIVE_FILE_BYTES / (1024 * 1024)
        ));
    }
    read_flash_archive_from(Cursor::new(bytes), budget)
}

/// Decode a seekable ZIP source using the caller's selection-level budget.
pub fn read_flash_archive_from<R: Read + Seek>(
    source: R,
    budget: &mut FlashArchiveBudget,
) -> Result<FlashImportSelection, String> {
    let mut archive =
        zip::ZipArchive::new(source).map_err(|error| format!("invalid ZIP archive: {error}"))?;
    if archive.len() > budget.remaining_entries {
        return Err(format!(
            "ZIP archive contains {} entries, but only {} remain in the selected-import limit",
            archive.len(),
            budget.remaining_entries
        ));
    }
    budget.remaining_entries -= archive.len();
    let mut selection = FlashImportSelection::default();
    for index in 0..archive.len() {
        let entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => {
                selection.push_error(format!("entry {index}: {error}"));
                continue;
            }
        };
        let archive_name = entry.name().to_owned();
        if entry.is_dir() || archive_name.ends_with(['/', '\\']) {
            continue;
        }
        let Some(name) = archive_name
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
        else {
            selection.push_error(format!("entry {index} has no filename"));
            continue;
        };
        let remaining_bytes = budget.remaining_bytes;
        let entry_limit = MAX_FLASH_ARCHIVE_ENTRY_BYTES.min(remaining_bytes);
        let mut bytes = Vec::new();
        let read_result = entry
            .take(entry_limit.saturating_add(1))
            .read_to_end(&mut bytes);
        let expanded_bytes = bytes.len() as u64;
        budget.remaining_bytes = budget.remaining_bytes.saturating_sub(expanded_bytes);
        match read_result {
            Err(error) => selection.push_error(format!("{archive_name}: {error}")),
            Ok(_) if expanded_bytes > entry_limit => {
                let message = if remaining_bytes <= MAX_FLASH_ARCHIVE_ENTRY_BYTES {
                    format!(
                        "selected ZIP import exceeds the {} MiB total uncompressed limit",
                        MAX_FLASH_ARCHIVE_TOTAL_BYTES / (1024 * 1024)
                    )
                } else {
                    format!(
                        "ZIP entry exceeds the {} MiB uncompressed limit",
                        MAX_FLASH_ARCHIVE_ENTRY_BYTES / (1024 * 1024)
                    )
                };
                selection.push_error(format!("{archive_name}: {message}"));
            }
            Ok(_) => selection.push_file(FlashDriveFile {
                name: name.to_owned(),
                bytes,
            }),
        }
    }
    if selection.files.is_empty() && selection.errors.is_empty() {
        return Err("ZIP archive contains no files".to_owned());
    }
    Ok(selection)
}
