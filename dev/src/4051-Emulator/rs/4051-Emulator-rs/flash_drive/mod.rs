//! Shared Flash Drive metadata, filename and archive header formats.
//!
//! Type/usage codes and labels are shared by machine LIST/filename handling and
//! archive interchange. Archive headers validate numbered entries and retain
//! descriptions as UTF-16 through the lossless JSON codec. Fixed-width metadata
//! preserves reference parsing quirks; host filenames additionally reject unsafe
//! separators, NUL and invalid Unicode. ZIP streams, filesystem operations and
//! tape selection/transfer state remain outside this module.

pub(crate) mod archive;
pub(crate) mod archive_json;

use self::archive_json::{ArchiveHeaderParser, json_utf16_string};
use std::{error::Error, fmt};

#[cfg(test)]
mod tests;

/// Why a numbered Flash Drive file cannot be exported to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlashDriveExportError {
    FileNumberOutOfRange(u8),
    FileNotFound(u8),
    InvalidFilename(u8),
}

impl fmt::Display for FlashDriveExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNumberOutOfRange(number) => {
                write!(
                    formatter,
                    "Flash Drive file number {number} is outside 1..=254"
                )
            }
            Self::FileNotFound(number) => {
                write!(formatter, "Flash Drive file {number} does not exist")
            }
            Self::InvalidFilename(number) => write!(
                formatter,
                "Flash Drive file {number} metadata cannot form a lossless host filename (invalid Unicode, path separator or NUL)"
            ),
        }
    }
}

impl Error for FlashDriveExportError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileType {
    Ascii,
    Binary,
    New,
    Last,
}

impl FileType {
    pub(crate) fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'A' => Some(Self::Ascii),
            b'B' => Some(Self::Binary),
            b'N' => Some(Self::New),
            b'L' => Some(Self::Last),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::Binary => "BINARY",
            Self::New => "NEW",
            Self::Last => "LAST",
        }
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn code(self) -> u8 {
        match self {
            Self::Ascii => b'A',
            Self::Binary => b'B',
            Self::New => b'N',
            Self::Last => b'L',
        }
    }
}

// Type and usage are independent: tFile.header preserves prior usage for NEW/LAST.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileUsage {
    Program,
    Data,
    Log,
    Text,
}

impl FileUsage {
    pub(crate) fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'P' => Some(Self::Program),
            b'D' => Some(Self::Data),
            b'L' => Some(Self::Log),
            b'T' => Some(Self::Text),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Program => "PROG",
            Self::Data => "DATA",
            Self::Log => "LOG",
            Self::Text => "TEXT",
        }
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn code(self) -> u8 {
        match self {
            Self::Program => b'P',
            Self::Data => b'D',
            Self::Log => b'L',
            Self::Text => b'T',
        }
    }
}

pub(crate) struct ParsedHeader {
    pub(crate) number: Option<u8>,
    pub(crate) file_type: FileType,
    pub(crate) usage: Option<FileUsage>,
    pub(crate) description: Vec<u16>,
}

pub(crate) fn metadata_units(
    number: Option<u8>,
    file_type: Option<FileType>,
    usage: Option<FileUsage>,
    description: &[u16],
    byte_len: usize,
) -> Option<Vec<u16>> {
    let number = number?;
    let file_type = file_type?;
    if (!matches!(file_type, FileType::New | FileType::Last) && usage.is_none())
        || description.len() > 16
    {
        return None;
    }
    let usage = usage.map_or(" ", FileUsage::label);
    let mut units: Vec<_> = format!("{number:<7}{:<8}{usage:<5}", file_type.label())
        .encode_utf16()
        .collect();
    units.extend(description);
    units.extend(std::iter::repeat_n(u16::from(b' '), 16 - description.len()));
    units.extend(format!(" {byte_len}").encode_utf16());
    Some(units)
}

pub(crate) fn encode_host_filename(
    requested_number: u8,
    number: Option<u8>,
    file_type: Option<FileType>,
    usage: Option<FileUsage>,
    description: &[u16],
    byte_len: usize,
) -> Result<String, FlashDriveExportError> {
    let units = metadata_units(number, file_type, usage, description, byte_len)
        .ok_or(FlashDriveExportError::InvalidFilename(requested_number))?;
    let name = String::from_utf16(&units)
        .map_err(|_| FlashDriveExportError::InvalidFilename(requested_number))?;
    if name
        .chars()
        .any(|character| matches!(character, '/' | '\\' | '\0'))
    {
        return Err(FlashDriveExportError::InvalidFilename(requested_number));
    }
    Ok(name)
}

// flashFilenameHandler checks only the first type/usage letters and ignores the size value.
// Description brackets are removed before its 16-character truncation and padding.
pub(crate) fn parse_header(units: &[u16]) -> Option<ParsedHeader> {
    let parsed_number = parse_integer(&units[..units.len().min(6)])?;
    let number = u8::try_from(parsed_number)
        .ok()
        .filter(|number| (1..=254).contains(number));
    let file_type = FileType::from_byte(u8::try_from(*units.get(7)?).ok()?)?;
    let usage = match file_type {
        FileType::Ascii | FileType::Binary => {
            Some(FileUsage::from_byte(u8::try_from(*units.get(15)?).ok()?)?)
        }
        FileType::New | FileType::Last => None,
    };
    let end = units
        .iter()
        .rposition(|&unit| unit == u16::from(b' '))
        .unwrap_or(0);
    let mut description: Vec<_> = units
        .iter()
        .skip(20)
        .take(end.saturating_sub(20))
        .copied()
        .filter(|&unit| unit != u16::from(b'[') && unit != u16::from(b']'))
        .take(16)
        .collect();
    description.resize(16, u16::from(b' '));
    Some(ParsedHeader {
        number,
        file_type,
        usage,
        description,
    })
}

// The reference's parseInt accepts leading whitespace/sign/hex and trailing non-digits.
pub(crate) fn parse_integer(units: &[u16]) -> Option<i32> {
    let mut input = units;
    while input.first().is_some_and(|unit| {
        matches!(*unit,
                0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a
                | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
    }) {
        input = &input[1..];
    }
    let negative = input.first() == Some(&u16::from(b'-'));
    if matches!(input.first(), Some(0x2b | 0x2d)) {
        input = &input[1..];
    }
    let radix = if input.starts_with(&[0x30, 0x78]) || input.starts_with(&[0x30, 0x58]) {
        input = &input[2..];
        16_i32
    } else {
        10_i32
    };
    let mut value = 0_i32;
    let mut digits = 0;
    for &unit in input {
        let digit = match unit {
            0x30..=0x39 => i32::from(unit - 0x30),
            0x61..=0x66 => i32::from(unit - 0x61 + 10),
            0x41..=0x46 => i32::from(unit - 0x41 + 10),
            _ => break,
        };
        if digit >= radix {
            break;
        }
        digits += 1;
        value = value.checked_mul(radix)?.checked_add(digit)?;
    }
    if digits == 0 {
        return None;
    }
    Some(if negative { -value } else { value })
}

#[derive(Debug)]
pub(crate) struct ArchiveHeader {
    pub(crate) number: u8,
    pub(crate) file_type: FileType,
    pub(crate) usage: Option<FileUsage>,
    pub(crate) description: Vec<u16>,
}

pub(crate) fn parse_archive_header(bytes: &[u8]) -> Result<ArchiveHeader, String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| format!("archive header is not UTF-8 JSON: {error}"))?;
    let mut parser = ArchiveHeaderParser::new(source);
    parser.consume(b'[')?;
    let number = parser.parse_number()?;
    if !(1..=254).contains(&number) {
        return Err(format!(
            "archive header file number {number} is outside 1..=254"
        ));
    }
    parser.consume(b',')?;
    let type_code = parser.parse_string()?;
    let [type_unit] = type_code.as_slice() else {
        return Err("archive header type must be one file-type code".to_owned());
    };
    let type_code =
        u8::try_from(*type_unit).map_err(|_| "archive header type code is not ASCII".to_owned())?;
    let file_type = FileType::from_byte(type_code)
        .ok_or_else(|| format!("archive header has unsupported file type {type_code:?}"))?;

    parser.consume(b',')?;
    let usage_code = parser.parse_string()?;
    let usage = match usage_code.as_slice() {
        [] => None,
        [unit] => {
            let code = u8::try_from(*unit)
                .map_err(|_| "archive header usage code is not ASCII".to_owned())?;
            Some(
                FileUsage::from_byte(code)
                    .ok_or_else(|| format!("archive header has unsupported usage code {code:?}"))?,
            )
        }
        _ => return Err("archive header usage must be empty or one usage code".to_owned()),
    };

    parser.consume(b',')?;
    let description = parser.parse_string()?;
    if description.len() > 16 {
        return Err("archive header description exceeds 16 UTF-16 units".to_owned());
    }
    parser.consume(b']')?;
    parser.finish()?;
    Ok(ArchiveHeader {
        number: number as u8,
        file_type,
        usage,
        description,
    })
}

pub(crate) fn encode_archive_header(
    number: u8,
    file_type: Option<FileType>,
    usage: Option<FileUsage>,
    description: &[u16],
) -> Result<String, String> {
    let file_type = match file_type {
        Some(FileType::Ascii) => "A",
        Some(FileType::Binary) => "B",
        Some(FileType::New) => "N",
        Some(FileType::Last) => "L",
        None => return Err(format!("Flash Drive file {number} has no type")),
    };
    let usage = match usage {
        Some(FileUsage::Program) => "P",
        Some(FileUsage::Data) => "D",
        Some(FileUsage::Log) => "L",
        Some(FileUsage::Text) => "T",
        None => "",
    };
    Ok(format!(
        "[{number},\"{file_type}\",\"{usage}\",{}]",
        json_utf16_string(description)
    ))
}
