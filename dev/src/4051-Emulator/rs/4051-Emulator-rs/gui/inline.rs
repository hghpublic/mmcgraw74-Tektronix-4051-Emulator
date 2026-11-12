//! Deterministic host adapters for the native widget regressions.
//!
//! The inline backend uses the production session state and command handler;
//! injected dialogs retain precise elapsed-time and file-result assertions.

use std::ops::{Deref, DerefMut};

use crate::emulation::CpuReferenceRenderer;
use crate::flash_drive_import::FlashImportOrder;

use super::super::super::SessionData;
use super::super::native_backend::InlineBackend;
use super::*;

pub(in super::super) type InlineApp = Tek4051App<InlineBackend>;

impl Deref for InlineApp {
    type Target = SessionData;

    fn deref(&self) -> &Self::Target {
        &self.backend.state.data
    }
}

impl DerefMut for InlineApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.backend.state.data
    }
}

impl InlineApp {
    pub(crate) fn cancel_auto_load(&mut self) {
        self.backend.state.cancel_auto_load_input();
    }

    pub(crate) fn new(context: &eframe::CreationContext<'_>, machine: Machine) -> Self {
        Self::with_backend(context, InlineBackend::new(machine))
            .expect("inline state must provide an initial snapshot")
    }

    pub(crate) fn update_display_texture(&mut self) {
        self.refresh();
    }

    pub(crate) fn display(&mut self) -> &CpuReferenceRenderer {
        let data = &mut self.backend.state.data;
        data.runtime.display(&mut data.machine)
    }

    pub(crate) fn update_at_with_dialogs(
        &mut self,
        context: &egui::Context,
        now: Instant,
        save: &mut impl FnMut(&[u8]) -> Result<Option<PathBuf>, String>,
        import: &mut impl FnMut() -> Option<FlashImportSelection>,
        export: &mut impl FnMut(FlashExportRequest<'_>) -> Result<Option<PathBuf>, String>,
        clock: &mut impl FnMut() -> Instant,
    ) {
        let mut confirm = confirm_flash_storage_archive_replacement;
        let mut pick = pick_flash_storage_archive;
        let mut imports = FlashImportDialogs {
            files: import,
            storage_archive_confirm: &mut confirm,
            storage_archive_pick: &mut pick,
        };
        self.update_at_with_import_dialogs(context, now, save, &mut imports, export, clock);
    }

    pub(crate) fn update_at_with_import_dialogs(
        &mut self,
        context: &egui::Context,
        now: Instant,
        save: &mut impl FnMut(&[u8]) -> Result<Option<PathBuf>, String>,
        imports: &mut FlashImportDialogs<'_>,
        export: &mut impl FnMut(FlashExportRequest<'_>) -> Result<Option<PathBuf>, String>,
        clock: &mut impl FnMut() -> Instant,
    ) {
        self.begin_frame(context, now);
        let requests = self.render(context);
        self.refresh();
        let mut copies: Vec<Vec<u8>> = self
            .snapshot
            .screen_copy_requests
            .iter()
            .map(|(_, pixels)| pixels.to_vec())
            .collect();
        let ids: Vec<_> = self
            .snapshot
            .screen_copy_requests
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.send(Command::CancelScreenCopy(id));
        }
        let data = &mut self.backend.state.data;
        let (mut pending, dropped_copies) = data.runtime.take_screen_copies(&mut data.machine);
        copies.append(&mut pending);
        let import_requested = requests.import;
        let import_storage_archive_requested = requests.import_archive;
        let export_requested = requests.export;
        let archive_export_requested = requests.export_archive;
        if import_requested {
            self.import_notices.clear();
            match (imports.files)() {
                None => self
                    .import_notices
                    .push((false, "Flash Drive import cancelled".to_owned())),
                Some(selection) => {
                    if let Err(error) = selection.validate_order() {
                        self.import_notices
                            .push((true, format!("Flash Drive import failed: {error}")));
                    } else {
                        let results = self.machine.import_flash_files(&selection.files);
                        let count = results.iter().filter(|result| result.is_ok()).count();
                        self.import_notices
                            .push((count == 0, format!("{count} Flash Drive file(s) imported")));
                        for item in selection.order {
                            match item {
                                FlashImportOrder::File(index) => {
                                    if let Err(error) = &results[index] {
                                        self.import_notices.push((
                                            true,
                                            format!(
                                                "Import failed for {}: {error}",
                                                selection.files[index].name
                                            ),
                                        ));
                                    }
                                }
                                FlashImportOrder::Error(index) => self
                                    .import_notices
                                    .push((true, selection.errors[index].clone())),
                            }
                        }
                    }
                }
            }
        }
        if import_storage_archive_requested {
            self.import_notices.clear();
            let notice = match import_flash_storage_archive_dialog(
                &mut self.machine,
                imports.storage_archive_confirm,
                imports.storage_archive_pick,
            ) {
                Ok(Some(path)) => (
                    false,
                    format!(
                        "Flash Drive storage replaced from archive: {} ({} file(s))",
                        path.display(),
                        self.machine.flash_drive_directory().len()
                    ),
                ),
                Ok(None) => (false, "Flash Drive archive import cancelled".to_owned()),
                Err(error) => (true, format!("Flash Drive archive import failed: {error}")),
            };
            self.import_notices.push(notice);
            context.request_repaint();
        }
        let mut export_dialog_opened = false;
        if export_requested {
            self.export_notice = Some(match self.machine.export_flash_file(self.export_number) {
                Ok(file) => {
                    export_dialog_opened = true;
                    match export(FlashExportRequest::File(&file)) {
                        Ok(Some(path)) => (
                            false,
                            format!(
                                "Flash Drive file {} exported: {}",
                                self.export_number,
                                path.display()
                            ),
                        ),
                        Ok(None) => (
                            false,
                            format!("Flash Drive file {} export cancelled", self.export_number),
                        ),
                        Err(error) => (true, format!("Flash Drive export failed: {error}")),
                    }
                }
                Err(error) => (true, format!("Flash Drive export failed: {error}")),
            });
            context.request_repaint();
        }
        let mut archive_export_dialog_opened = false;
        if archive_export_requested {
            if self.machine.flash_drive_directory().is_empty() {
                self.export_notice = Some((
                    true,
                    "Flash Drive archive export failed: Flash Drive contains no files".to_owned(),
                ));
            } else {
                archive_export_dialog_opened = true;
                self.export_notice =
                    Some(match export(FlashExportRequest::Archive(&self.machine)) {
                        Ok(Some(path)) => (
                            false,
                            format!("Flash Drive archive exported: {}", path.display()),
                        ),
                        Ok(None) => (false, "Flash Drive archive export cancelled".to_owned()),
                        Err(error) => (true, format!("Flash Drive archive export failed: {error}")),
                    });
            }
            context.request_repaint();
        }
        let dialogs_opened = import_requested
            || import_storage_archive_requested
            || export_dialog_opened
            || archive_export_dialog_opened
            || !copies.is_empty();
        if !copies.is_empty() || dropped_copies != 0 {
            self.screen_copy_notices.clear();
            if dropped_copies != 0 {
                self.screen_copy_notices.push((
                    true,
                    format!("At least {dropped_copies} screen copy request(s) could not be saved: queue full"),
                ));
            }
            for pixels in copies {
                self.screen_copy_notices.push(match save(&pixels) {
                    Ok(Some(path)) => (false, format!("Screen copy saved: {}", path.display())),
                    Ok(None) => (false, "Screen copy cancelled".to_owned()),
                    Err(error) => (true, format!("Screen copy failed: {error}")),
                });
            }
        }
        if dialogs_opened {
            // Modal time pauses interval work, but independent erase timeouts still expire.
            let after_dialog = clock();
            let state = &mut self.backend.state.data;
            state.runtime.advance_erase_callbacks(
                &mut state.machine,
                after_dialog.saturating_duration_since(now),
            );
            self.last_update = after_dialog;
            self.update_display_texture();
            context.request_repaint();
        }
        self.refresh();
    }
}
