//! Dialog callbacks and a speed widget used only by private native regressions.
//!
//! Borrowed selections keep the existing inline file/clock assertions intact;
//! production startup and file selection do not load this module.

use std::{fs::File, path::PathBuf};

use eframe::egui;
use tek4051_emulator::{
    emulation::SPEED_FACTORS,
    host_runtime::HostRuntime,
    machine::{Machine, gpib::FlashDriveFile},
};

use super::FlashImportSelection;

pub(super) struct FlashImportDialogs<'a> {
    pub(super) files: &'a mut dyn FnMut() -> Option<FlashImportSelection>,
    pub(super) storage_archive_confirm: &'a mut dyn FnMut() -> bool,
    pub(super) storage_archive_pick: &'a mut dyn FnMut() -> Option<PathBuf>,
}

pub(super) enum FlashExportRequest<'a> {
    File(&'a FlashDriveFile),
    Archive(&'a Machine),
}

pub(super) fn import_flash_storage_archive_dialog(
    machine: &mut Machine,
    confirm_replacement: &mut dyn FnMut() -> bool,
    pick_archive: &mut dyn FnMut() -> Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    if !confirm_replacement() {
        return Ok(None);
    }
    let Some(path) = pick_archive() else {
        return Ok(None);
    };
    let file =
        File::open(&path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    machine
        .import_flash_storage_archive(file)
        .map_err(|error| format!("cannot import {}: {error}", path.display()))?;
    Ok(Some(path))
}

pub(super) fn speed_control(
    ui: &mut egui::Ui,
    runtime: &mut HostRuntime,
    speed_index: &mut usize,
    settings_error: &mut Option<String>,
) -> egui::Response {
    let response = ui.button(format!("x{}", SPEED_FACTORS[*speed_index]));
    if response.clicked() {
        let next_index = (*speed_index + 1) % SPEED_FACTORS.len();
        match runtime.set_speed_factor(f64::from(SPEED_FACTORS[next_index])) {
            Ok(()) => {
                *speed_index = next_index;
                *settings_error = None;
            }
            Err(error) => *settings_error = Some(error.to_string()),
        }
    }
    response
}
