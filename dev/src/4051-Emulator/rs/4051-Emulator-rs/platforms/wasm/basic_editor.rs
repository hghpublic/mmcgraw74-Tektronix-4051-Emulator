//! Rust-side source preparation for the browser BASIC editor.
//!
//! The browser owns text editing and file-picker APIs. This module validates
//! numbered BASIC source, produces CR-terminated ASCII records, and builds
//! canonical Flash Drive-importable `.bas` filenames.

use wasm_bindgen::prelude::*;

use crate::flash_drive::{FileType, FileUsage, encode_host_filename, parse_header};

#[cfg(test)]
#[path = "basic_editor/tests.rs"]
mod tests;

pub(crate) fn numbered_source_lines(source: &str) -> Result<Vec<String>, String> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lines = Vec::new();
    let mut line_numbers = std::collections::BTreeSet::new();

    for (index, line) in source.split(['\r', '\n']).enumerate() {
        let line = line.trim_matches(' ');
        if line.is_empty() {
            continue;
        }
        if !line.is_ascii() || line.chars().any(char::is_control) {
            return Err(format!(
                "BASIC source line {} must use printable ASCII",
                index + 1
            ));
        }

        let (number, statement) = line.split_once(' ').ok_or_else(|| {
            format!(
                "BASIC source line {} needs a line number and statement",
                index + 1
            )
        })?;
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!(
                "BASIC source line {} must start with a line number",
                index + 1
            ));
        }
        if statement.trim().is_empty() {
            return Err(format!("BASIC source line {} has no statement", index + 1));
        }
        let number = number
            .parse::<u32>()
            .map_err(|_| format!("BASIC source line {} has an invalid line number", index + 1))?;
        if !line_numbers.insert(number) {
            return Err(format!("BASIC line number {number} appears more than once"));
        }
        lines.push(line.to_owned());
    }

    if lines.is_empty() {
        return Err("BASIC program is empty".to_owned());
    }
    Ok(lines)
}

pub(crate) fn program_file_bytes(source: &str) -> Result<Vec<u8>, String> {
    let lines = numbered_source_lines(source)?;
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\r');
    }
    Ok(bytes)
}

pub(crate) fn prepare_file(
    requested_name: &str,
    source: &str,
    used_file_numbers: impl IntoIterator<Item = u8>,
) -> Result<BrowserBasicFile, String> {
    let bytes = program_file_bytes(source)?;
    let number = first_available_file_number(used_file_numbers)?;
    let description = file_description(requested_name)?;
    let encoded_description: Vec<_> = description.encode_utf16().collect();
    let name = encode_host_filename(
        number,
        Some(number),
        Some(FileType::Ascii),
        Some(FileUsage::Program),
        &encoded_description,
        bytes.len(),
    )
    .map_err(|error| error.to_string())?;

    Ok(BrowserBasicFile {
        name: format!("{name}.bas"),
        bytes,
    })
}

fn first_available_file_number(
    used_file_numbers: impl IntoIterator<Item = u8>,
) -> Result<u8, String> {
    let mut used = [false; 255];
    for number in used_file_numbers {
        if (1..=254).contains(&number) {
            used[usize::from(number)] = true;
        }
    }
    (1..=254)
        .find(|&number| !used[usize::from(number)])
        .ok_or_else(|| {
            "No Flash Drive file numbers are available for this BASIC program".to_owned()
        })
}

fn file_description(requested_name: &str) -> Result<String, String> {
    let requested_name = requested_name.trim();
    let stem = strip_bas_extension(requested_name);
    let existing_description = parse_header(&stem.encode_utf16().collect::<Vec<_>>())
        .and_then(|header| String::from_utf16(&header.description).ok())
        .map(|description| description.trim().to_owned());
    let source = existing_description
        .filter(|description| !description.is_empty())
        .unwrap_or_else(|| stem.to_owned());

    let mut sanitized = String::new();
    for character in source.chars() {
        if character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
        {
            sanitized.push('_');
        } else {
            sanitized.push(character);
        }
    }
    let sanitized = sanitized.trim().trim_end_matches('.').trim().to_owned();
    if sanitized.is_empty() {
        return Err("Enter a nonempty BASIC program filename".to_owned());
    }

    let mut description = String::new();
    let mut units = 0;
    for character in sanitized.chars() {
        let character_units = character.len_utf16();
        if units + character_units > 16 {
            break;
        }
        units += character_units;
        description.push(character);
    }
    if description.is_empty() {
        return Err("The BASIC program filename has no usable characters".to_owned());
    }
    Ok(description)
}

fn strip_bas_extension(name: &str) -> &str {
    name.rsplit_once('.')
        .filter(|(_, extension)| extension.eq_ignore_ascii_case("bas"))
        .map_or(name, |(stem, _)| stem)
}

/// A prepared ASCII BASIC program and its canonical host filename.
#[wasm_bindgen]
pub struct BrowserBasicFile {
    name: String,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl BrowserBasicFile {
    /// Returns the canonical `.bas` filename, including Flash Drive metadata.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Returns CR-terminated ASCII source bytes suitable for Flash Drive import.
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}
