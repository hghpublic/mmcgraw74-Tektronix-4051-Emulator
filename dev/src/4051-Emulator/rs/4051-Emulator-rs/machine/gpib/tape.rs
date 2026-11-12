//! The implemented subset of the default 4924 tape device.
//!
//! Numbered files, their type/usage/description, and NEW/LAST markers are held in memory.
//! FIND selects a snapshot for OLD; SAVE updates the numbered store without replacing
//! that snapshot. PRINT updates current file contents using the shared record
//! cursor; INPUT walks CR-terminated records in the separate FIND snapshot.
//! WRITE commits individual bytes; READ supplies current binary contents once per rewind.
//! BOLD supplies the binary-program FIND snapshot with the reference's block padding.
//! LIST reads and writes fixed-width metadata without refreshing the FIND snapshot or cursor.
//! Header renumbering leaves the separate FIND number used by SAVE/BSAVE unchanged.
//! Flash Drive imports reuse filename parsing, retain successful files, and select
//! the first stored number without rewinding the transfer snapshot.
//! Host directory snapshots and export read canonical stored files without changing transfer state.
//! Whole-storage archives stream ordered JSON headers and raw contents without changing transfer state.
//! Whole-store replacement clears snapshots tied to old files; archive replacement
//! validates all pairs before committing.
//! Shared codecs handle ZIP, headers and filenames; LIST byte conversion/state policy stays here.
//! Unimplemented commands remain errors at dispatch.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    io::{Read, Seek, Write},
};

use super::GpibBuffers;
#[cfg(feature = "wasm")]
use crate::flash_drive::archive::OwnedArchiveFile;
use crate::flash_drive::archive::{ArchiveFileRef, read_archive, write_archive};
use crate::flash_drive::{
    FileType, FileUsage, FlashDriveExportError, encode_host_filename, metadata_units, parse_header,
    parse_integer,
};

#[cfg(any(feature = "wasm", test))]
const EDITOR_PROGRAM_NUMBER: u8 = 254;

#[cfg(test)]
#[path = "tape/tests.rs"]
mod tests;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TapePrintError {
    FileNotSelected,
    PositionOverflow,
}

pub(super) enum TapeHeaderError {
    Invalid(Vec<u8>),
    FileNotSelected,
}

/// A tape program number outside the reference's supported range, 1 through 254.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapeFileNumberError(pub u8);

impl fmt::Display for TapeFileNumberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "tape file number {} is outside 1..=254", self.0)
    }
}

impl Error for TapeFileNumberError {}

/// Flash Drive filename and unmodified file contents for host import/export.
#[derive(Debug)]
pub struct FlashDriveFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Canonical numbered-file metadata without a copy of its contents.
///
/// Description units retain the stored UTF-16, including padding and split surrogates.
/// Missing type/usage values remain explicit; NEW/LAST may have no usage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashDriveFileInfo {
    pub number: u8,
    pub file_type: Option<&'static str>,
    pub usage: Option<&'static str>,
    pub description: Vec<u16>,
    pub byte_len: usize,
}

/// A rejected Flash Drive filename; other files in an import batch can still succeed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashDriveImportError {
    InvalidFilename,
    FileNumberOutOfRange,
}

impl fmt::Display for FlashDriveImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFilename => write!(formatter, "invalid Flash Drive filename"),
            Self::FileNumberOutOfRange => {
                write!(formatter, "Flash Drive file number is outside 1..=254")
            }
        }
    }
}

impl Error for FlashDriveImportError {}

#[derive(Clone, Default)]
struct TapeFile {
    number: Option<u8>,
    file_type: Option<FileType>,
    usage: Option<FileUsage>,
    // Filename descriptions use JavaScript UTF-16 units; GPIB maps each unit to its low byte.
    description: Vec<u16>,
    bytes: Vec<u8>,
}

#[derive(Clone)]
struct TapeSelection {
    selected_bytes: Vec<u8>,
    selected_number: Option<u8>,
    current_file: TapeFile,
    read_position: usize,
    clear_before_write: bool,
}

impl TapeFile {
    // LIST uses the low byte of these units; host filenames retain the full Unicode description.
    fn filename_units(&self) -> Option<Vec<u16>> {
        metadata_units(
            self.number,
            self.file_type,
            self.usage,
            &self.description,
            self.bytes.len(),
        )
    }
}

#[derive(Default)]
pub(super) struct Tape {
    files: BTreeMap<u8, TapeFile>,
    editor_program: Option<TapeFile>,
    editor_selection: Option<TapeSelection>,
    selected_bytes: Vec<u8>,
    // The fileNum control used by SAVE/BSAVE; LIST does not update that control.
    selected_number: Option<u8>,
    // A missing FIND retains OLD bytes but selects an untyped current file.
    current_file: TapeFile,
    // INPUT, PRINT, and READ share this cursor; it can exceed the FIND snapshot.
    read_position: usize,
    // FIND of data arms a one-byte WRITE clear; other FINDs and CLOSE do not disarm it.
    clear_before_write: bool,
}

impl Tape {
    #[cfg(any(feature = "wasm", test))]
    pub(super) fn stage_editor_program(&mut self, bytes: &[u8]) -> u8 {
        self.clear_editor_program();
        // FIND resolves this private entry before any same-numbered user file.
        let number = EDITOR_PROGRAM_NUMBER;
        self.editor_program = Some(TapeFile {
            number: Some(number),
            file_type: Some(FileType::Ascii),
            usage: Some(FileUsage::Program),
            bytes: bytes.to_vec(),
            ..TapeFile::default()
        });
        number
    }

    pub(super) fn clear_editor_program(&mut self) {
        self.editor_program = None;
        self.clear_editor_selection();
    }

    fn clear_editor_selection(&mut self) {
        if let Some(selection) = self.editor_selection.take() {
            self.selected_bytes = selection.selected_bytes;
            self.selected_number = selection.selected_number;
            self.current_file = selection.current_file;
            self.read_position = selection.read_position;
            self.clear_before_write = selection.clear_before_write;
        }
    }

    pub(super) fn flash_drive_directory(&self) -> Vec<FlashDriveFileInfo> {
        self.files
            .iter()
            .map(|(&number, file)| FlashDriveFileInfo {
                number,
                file_type: file.file_type.map(FileType::label),
                usage: file.usage.map(FileUsage::label),
                description: file.description.clone(),
                byte_len: file.bytes.len(),
            })
            .collect()
    }

    pub(super) fn write_flash_storage_archive<W: Write + Seek>(
        &self,
        destination: W,
    ) -> Result<W, String> {
        write_archive(
            destination,
            self.files.iter().map(|(&number, file)| ArchiveFileRef {
                number,
                file_type: file.file_type,
                usage: file.usage,
                description: &file.description,
                bytes: &file.bytes,
            }),
        )
    }

    #[cfg(feature = "wasm")]
    pub(super) fn flash_drive_archive_file(&self, number: u8) -> Option<OwnedArchiveFile> {
        self.files
            .get(&number)
            .map(|file| Self::owned_archive_file(number, file))
    }

    pub(super) fn import_flash_storage_archive<R: Read + Seek>(
        &mut self,
        source: R,
    ) -> Result<(), String> {
        let files: BTreeMap<_, _> = read_archive(source)?
            .into_iter()
            .map(|(number, file)| {
                (
                    number,
                    TapeFile {
                        number: Some(number),
                        file_type: Some(file.header.file_type),
                        usage: file.header.usage,
                        description: file.header.description,
                        bytes: file.bytes,
                    },
                )
            })
            .collect();
        self.replace_flash_drive_files(files);
        Ok(())
    }

    pub(super) fn clear_flash_drive(&mut self) {
        self.replace_flash_drive_files(BTreeMap::new());
    }

    // Drop every selection, snapshot, and cursor that could still reference replaced storage.
    fn replace_flash_drive_files(&mut self, files: BTreeMap<u8, TapeFile>) {
        self.clear_editor_program();
        self.selected_number = files.first_key_value().map(|(number, _)| *number);
        self.current_file = files
            .first_key_value()
            .map_or_else(TapeFile::default, |(_, file)| file.clone());
        self.selected_bytes.clear();
        self.read_position = 0;
        self.clear_before_write = false;
        self.files = files;
    }

    #[cfg(feature = "wasm")]
    fn owned_archive_file(number: u8, file: &TapeFile) -> OwnedArchiveFile {
        OwnedArchiveFile {
            number,
            file_type: file.file_type,
            usage: file.usage,
            description: file.description.clone(),
            bytes: file.bytes.clone(),
        }
    }

    pub(super) fn export_flash_file(
        &self,
        number: u8,
    ) -> Result<FlashDriveFile, FlashDriveExportError> {
        let name = self.export_flash_file_name(number)?;
        Ok(FlashDriveFile {
            name,
            bytes: self.files[&number].bytes.clone(),
        })
    }

    pub(super) fn export_flash_file_name(
        &self,
        number: u8,
    ) -> Result<String, FlashDriveExportError> {
        if !(1..=254).contains(&number) {
            return Err(FlashDriveExportError::FileNumberOutOfRange(number));
        }
        let file = self
            .files
            .get(&number)
            .ok_or(FlashDriveExportError::FileNotFound(number))?;
        encode_host_filename(
            number,
            file.number,
            file.file_type,
            file.usage,
            &file.description,
            file.bytes.len(),
        )
    }

    pub(super) fn import_flash_files(
        &mut self,
        files: &[FlashDriveFile],
    ) -> Vec<Result<(), FlashDriveImportError>> {
        if files.is_empty() {
            return Vec::new();
        }
        self.clear_editor_program();
        self.selected_number = None;
        let results = files
            .iter()
            .map(|file| {
                let units: Vec<_> = file.name.encode_utf16().collect();
                let header = parse_header(&units).ok_or(FlashDriveImportError::InvalidFilename)?;
                let number = header
                    .number
                    .ok_or(FlashDriveImportError::FileNumberOutOfRange)?;
                self.files.insert(
                    number,
                    TapeFile {
                        number: Some(number),
                        file_type: Some(header.file_type),
                        usage: header.usage,
                        description: header.description,
                        bytes: file.bytes.clone(),
                    },
                );
                Ok(())
            })
            .collect();
        // importFlashDir selects the first entry in the whole sorted store, not just this batch.
        if let Some((_, file)) = self.files.first_key_value() {
            self.current_file.clone_from(file);
            self.selected_number = file.number;
        }
        results
    }

    pub(super) fn store_program(
        &mut self,
        number: u8,
        bytes: &[u8],
    ) -> Result<(), TapeFileNumberError> {
        if !(1..=254).contains(&number) {
            return Err(TapeFileNumberError(number));
        }
        self.files.insert(
            number,
            TapeFile {
                number: Some(number),
                file_type: Some(FileType::Ascii),
                usage: Some(FileUsage::Program),
                bytes: bytes.to_vec(),
                ..TapeFile::default()
            },
        );
        Ok(())
    }

    pub(super) fn find(&mut self, buffers: &mut GpibBuffers) -> Result<(), Vec<u8>> {
        let output = buffers.take_output();
        buffers.clear_input();
        let number = parse_file_number(&output).ok_or_else(|| output.clone())?;
        self.clear_editor_selection();
        if let Some(file) = self
            .editor_program
            .as_ref()
            .filter(|file| file.number == Some(number))
            .cloned()
        {
            self.editor_selection = Some(TapeSelection {
                selected_bytes: self.selected_bytes.clone(),
                selected_number: self.selected_number,
                current_file: self.current_file.clone(),
                read_position: self.read_position,
                clear_before_write: self.clear_before_write,
            });
            self.selected_number = Some(number);
            self.current_file = file.clone();
            self.selected_bytes.clone_from(&file.bytes);
            self.read_position = 0;
            self.clear_before_write = false;
            return Ok(());
        }
        self.selected_number = Some(number);
        match self.files.get(&number) {
            Some(file) => {
                self.current_file.clone_from(file);
                if matches!(file.file_type, Some(FileType::Ascii | FileType::Binary)) {
                    self.selected_bytes.clone_from(&file.bytes);
                    self.read_position = 0;
                    if file.usage == Some(FileUsage::Data) {
                        self.clear_before_write = true;
                    }
                }
            }
            None => {
                self.current_file = TapeFile {
                    number: Some(number),
                    ..TapeFile::default()
                };
            }
        }
        Ok(())
    }

    pub(super) fn save(
        &mut self,
        buffers: &mut GpibBuffers,
        file_type: FileType,
    ) -> Result<(), ()> {
        let output = buffers.take_output();
        let number = self.selected_number.ok_or(())?;
        self.files.insert(
            number,
            TapeFile {
                number: Some(number),
                file_type: Some(file_type),
                usage: Some(FileUsage::Program),
                bytes: output,
                ..TapeFile::default()
            },
        );
        Ok(())
    }

    pub(super) fn old(&mut self, buffers: &mut GpibBuffers) {
        buffers.replace_input(self.selected_bytes.clone());
        if self.editor_selection.is_some() {
            self.clear_editor_program();
        }
    }

    pub(super) fn bold(&self, buffers: &mut GpibBuffers) {
        if self.current_file.file_type == Some(FileType::Binary)
            && self.current_file.usage == Some(FileUsage::Program)
        {
            // Aligned programs still receive a full extra block, beginning with 0xff.
            let padding = 256 - self.selected_bytes.len() % 256;
            let mut bytes = self.selected_bytes.clone();
            bytes.push(0xff);
            bytes.extend(std::iter::repeat_n(b' ', padding - 1));
            buffers.replace_input(bytes);
        }
    }

    pub(super) fn print(&mut self, buffers: &mut GpibBuffers) -> Result<(), TapePrintError> {
        let output = buffers.take_output();
        let number = self
            .current_file
            .number
            .ok_or(TapePrintError::FileNotSelected)?;
        if matches!(
            self.current_file.file_type,
            Some(FileType::Binary | FileType::Last)
        ) {
            return Ok(());
        }
        let bytes = if self.read_position == 0 {
            output
        } else {
            let mut bytes = self.current_file.bytes.clone();
            bytes.extend_from_slice(&output);
            bytes
        };
        // gpibPrint advances by the whole resulting buffer, not just newly appended bytes.
        let position = self
            .read_position
            .checked_add(bytes.len())
            .ok_or(TapePrintError::PositionOverflow)?;
        self.current_file.bytes = bytes;
        self.read_position = position;
        if matches!(self.current_file.file_type, None | Some(FileType::New)) {
            self.current_file.file_type = Some(FileType::Ascii);
            self.current_file.usage = Some(FileUsage::Data);
        }
        self.files.insert(number, self.current_file.clone());
        Ok(())
    }

    pub(super) fn input(&mut self, buffers: &mut GpibBuffers) {
        if self.current_file.file_type != Some(FileType::Ascii) {
            buffers.replace_input(vec![0xff]);
            return;
        }

        let start = self.read_position;
        // PRINT can move this shared cursor beyond the retained FIND snapshot.
        if start >= self.selected_bytes.len() {
            return;
        }
        self.read_position = self.selected_bytes.len();
        if let Some(offset) = self.selected_bytes[start..]
            .iter()
            .position(|&byte| byte == b'\r')
        {
            self.read_position = start + offset + 1;
            buffers.replace_input(self.selected_bytes[start..self.read_position].to_vec());
        }
        // Like gpibInput, EOF or an unterminated tail leaves the shared input unchanged.
    }

    pub(super) fn read(&mut self, buffers: &mut GpibBuffers) {
        if self.current_file.file_type != Some(FileType::Binary) {
            buffers.replace_input(vec![0xff]);
        } else if self.read_position == 0 {
            // gpibRead uses 99 as a consumed marker, not a byte offset into the binary file.
            self.read_position = 99;
            buffers.replace_input(self.current_file.bytes.clone());
        }
    }

    pub(super) fn write(&mut self, buffers: &GpibBuffers) -> Result<(), ()> {
        let number = self.current_file.number.ok_or(())?;
        // The reference clears before its type guard, even for a rejected ASCII WRITE.
        if self.clear_before_write {
            self.clear_before_write = false;
            self.current_file.bytes.clear();
        }
        if matches!(
            self.current_file.file_type,
            Some(FileType::Ascii | FileType::Last)
        ) {
            return Ok(());
        }
        self.current_file.bytes.push(buffers.byte_to_send());
        if matches!(self.current_file.file_type, None | Some(FileType::New)) {
            self.current_file.file_type = Some(FileType::Binary);
            self.current_file.usage = Some(FileUsage::Data);
        }
        self.files.insert(number, self.current_file.clone());
        Ok(())
    }

    pub(super) fn list(&self, buffers: &mut GpibBuffers) {
        // LIST still describes the working file, not the canonical entry used by host export.
        let units = self
            .current_file
            .number
            .filter(|number| self.files.contains_key(number))
            .and_then(|_| self.current_file.filename_units());
        let bytes = units.map_or_else(
            || vec![0xff],
            |units| {
                units
                    .into_iter()
                    .map(|unit| unit.to_le_bytes()[0])
                    .collect()
            },
        );
        buffers.replace_input(bytes);
    }

    pub(super) fn write_header(
        &mut self,
        buffers: &mut GpibBuffers,
    ) -> Result<(), TapeHeaderError> {
        let output = buffers.take_output();
        let units: Vec<_> = output.iter().copied().map(u16::from).collect();
        let header = parse_header(&units).ok_or(TapeHeaderError::Invalid(output))?;
        // Out-of-range numeric headers pass the parser but tFile.num ignores their number.
        let number = header
            .number
            .or(self.current_file.number)
            .ok_or(TapeHeaderError::FileNotSelected)?;
        self.current_file.number = Some(number);
        self.current_file.file_type = Some(header.file_type);
        if let Some(usage) = header.usage {
            self.current_file.usage = Some(usage);
        }
        self.current_file.description = header.description;
        // Renumbering copies current contents to the new key; the old entry is not deleted.
        self.files.insert(number, self.current_file.clone());
        Ok(())
    }

    pub(super) fn close(&mut self) {
        self.clear_editor_program();
        self.selected_bytes.clear();
        self.selected_number = None;
        self.current_file = TapeFile::default();
    }
}

// FIND uses JavaScript parseInt: whitespace, sign, hexadecimal prefix, and
// trailing non-digits are accepted. Invalid numbers trigger a file dialog
// in the reference, which this subset cannot provide.
fn parse_file_number(bytes: &[u8]) -> Option<u8> {
    let units: Vec<_> = bytes.iter().copied().map(u16::from).collect();
    u8::try_from(parse_integer(&units)?)
        .ok()
        .filter(|number| (1..=254).contains(number))
}
