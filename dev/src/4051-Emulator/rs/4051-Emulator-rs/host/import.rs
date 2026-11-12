//! Prepares selected host files for import without changing machine state.
//!
//! Loose files and ZIP entries become owned Flash Drive inputs; ZIP limits are
//! shared across a batch and failures remain per file or archive. Folder imports
//! enumerate only immediate regular files and process them in sorted path order.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use tek4051_emulator::machine::gpib::FlashDriveFile;
#[cfg(test)]
pub(super) use tek4051_emulator::machine::gpib::{
    MAX_FLASH_ARCHIVE_ENTRIES, MAX_FLASH_ARCHIVE_ENTRY_BYTES, MAX_FLASH_ARCHIVE_TOTAL_BYTES,
};
pub(super) use tek4051_emulator::{
    flash_drive_import::{FlashArchiveBudget, FlashImportSelection, read_flash_archive_from},
    machine::gpib::MAX_FLASH_ARCHIVE_FILE_BYTES,
};

pub(super) fn read_flash_paths(paths: impl IntoIterator<Item = PathBuf>) -> FlashImportSelection {
    read_flash_paths_with_budget(paths, &mut FlashArchiveBudget::default())
}

pub(super) fn read_flash_directory(path: &Path) -> FlashImportSelection {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            let mut selection = FlashImportSelection::default();
            selection.push_error(format!(
                "Folder import failed for {}: {error}",
                path.display()
            ));
            return selection;
        }
    };
    let mut files = Vec::new();
    let mut listing_errors = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                listing_errors.push(format!(
                    "Folder import failed while reading {}: {error}",
                    path.display()
                ));
                continue;
            }
        };
        let entry_path = entry.path();
        let metadata = match std::fs::metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                listing_errors.push(format!(
                    "Folder import failed while inspecting {}: {error}",
                    entry_path.display()
                ));
                continue;
            }
        };
        if metadata.is_file() {
            files.push(entry_path);
        }
    }
    if files.is_empty() && listing_errors.is_empty() {
        listing_errors.push(format!(
            "Folder import failed: no files found in {}",
            path.display()
        ));
    }
    files.sort();
    let mut selection = read_flash_paths(files);
    for error in listing_errors {
        selection.push_error(error);
    }
    selection
}

pub(super) fn read_flash_paths_with_budget(
    paths: impl IntoIterator<Item = PathBuf>,
    budget: &mut FlashArchiveBudget,
) -> FlashImportSelection {
    let mut selection = FlashImportSelection::default();
    for path in paths {
        let is_zip = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
        if is_zip {
            match read_flash_archive(&path, budget) {
                Ok(archive) => {
                    selection.append(archive);
                }
                Err(error) => selection.push_error(error),
            }
        } else {
            match read_flash_file(&path) {
                Ok(file) => selection.push_file(file),
                Err(error) => selection.push_error(error),
            }
        }
    }
    selection
}

pub(super) fn read_flash_archive(
    path: &Path,
    budget: &mut FlashArchiveBudget,
) -> Result<FlashImportSelection, String> {
    let file = File::open(path)
        .map_err(|error| format!("Import failed for {}: {error}", path.display()))?;
    let file_size = file
        .metadata()
        .map_err(|error| format!("Import failed for {}: {error}", path.display()))?
        .len();
    if file_size > MAX_FLASH_ARCHIVE_FILE_BYTES {
        return Err(format!(
            "Import failed for {}: ZIP exceeds the {} MiB compressed file limit",
            path.display(),
            MAX_FLASH_ARCHIVE_FILE_BYTES / (1024 * 1024)
        ));
    }
    let mut selection = read_flash_archive_from(file, budget)
        .map_err(|error| format!("Import failed for {}: {error}", path.display()))?;
    for error in &mut selection.errors {
        *error = format!("Import failed for {} ({error})", path.display());
    }
    Ok(selection)
}

pub(super) fn read_flash_file(path: &Path) -> Result<FlashDriveFile, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "Import failed for {}: filename is not Unicode",
                path.display()
            )
        })?;
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Import failed for {}: {error}", path.display()))?;
    Ok(FlashDriveFile {
        name: name.to_owned(),
        bytes,
    })
}
