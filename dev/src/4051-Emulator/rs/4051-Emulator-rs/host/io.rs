//! Executes native file effects on the session owner, never during GUI repaint.
//!
//! Numbered exports and PNG saves use the data prepared by the session. Archive
//! reads/writes use the same suspended machine and existing streaming/validation
//! APIs. Every host failure becomes the existing operation-specific notice.

use std::{fs::File, path::Path};

use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use tek4051_emulator::{
    emulation::{CpuReferenceRenderer, EmulationState, FileEffect, FileOutcome},
    machine::gpib::FlashDriveFile,
};

pub(super) fn execute(effect: FileEffect, session: &mut EmulationState) -> FileOutcome {
    match effect {
        FileEffect::ImportArchive(path) => {
            let result = File::open(&path)
                .map_err(|error| format!("cannot open {}: {error}", path.display()))
                .and_then(|file| {
                    session
                        .import_archive(file)
                        .map_err(|error| format!("cannot import {}: {error}", path.display()))
                });
            FileOutcome::Import(match result {
                Ok(count) => (
                    false,
                    format!(
                        "Flash Drive storage replaced from archive: {} ({} file(s))",
                        path.display(),
                        count
                    ),
                ),
                Err(error) => (true, format!("Flash Drive archive import failed: {error}")),
            })
        }
        FileEffect::ExportFile { number, path, file } => {
            FileOutcome::Export(match write_flash_file(&path, &file) {
                Ok(()) => (
                    false,
                    format!("Flash Drive file {number} exported: {}", path.display()),
                ),
                Err(error) => (true, format!("Flash Drive export failed: {error}")),
            })
        }
        FileEffect::ExportArchive(path) => {
            FileOutcome::Export(match write_flash_storage_archive_to_path(&path, session) {
                Ok(()) => (
                    false,
                    format!("Flash Drive archive exported: {}", path.display()),
                ),
                Err(error) => (true, format!("Flash Drive archive export failed: {error}")),
            })
        }
        FileEffect::SaveScreen { path, pixels } => {
            FileOutcome::Screen(match save_screen(&path, &pixels) {
                Ok(()) => (false, format!("Screen copy saved: {}", path.display())),
                Err(error) => (true, format!("Screen copy failed: {error}")),
            })
        }
    }
}

pub(super) fn write_flash_file(path: &Path, file: &FlashDriveFile) -> Result<(), String> {
    std::fs::write(path, &file.bytes)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

pub(super) fn write_flash_storage_archive_to_path(
    path: &Path,
    session: &EmulationState,
) -> Result<(), String> {
    let file =
        File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    session
        .write_archive(file)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(())
}

pub(super) fn save_screen(path: &Path, pixels: &[u8]) -> Result<(), image::ImageError> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &intensity in pixels {
        rgba.extend_from_slice(&[0, intensity, 0, u8::MAX]);
    }
    PngEncoder::new(File::create(path)?).write_image(
        &rgba,
        CpuReferenceRenderer::WIDTH as u32,
        CpuReferenceRenderer::HEIGHT as u32,
        ExtendedColorType::Rgba8,
    )
}
