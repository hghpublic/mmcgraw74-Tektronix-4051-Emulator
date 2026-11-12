//! Native file and confirmation dialogs used by background host jobs.
//!
//! The presentation requests these choices after suspending emulation; the
//! returned paths are handled by the separate host import/effect adapters.

use std::path::PathBuf;

pub(super) fn pick_flash_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select folder containing 4050 Flash Drive files or ZIPs")
        .pick_folder()
}

pub(super) fn confirm_flash_storage_archive_replacement() -> bool {
    rfd::MessageDialog::new()
        .set_title("Replace Flash Drive storage?")
        .set_description(
            "WARNING: This replaces all current files and aborts any currently addressed tape transfer. Continue?",
        )
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

pub(super) fn pick_flash_storage_archive() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import 4050 Flash Drive archive")
        .add_filter("ZIP archive", &["zip"])
        .pick_file()
}

pub(super) fn save_flash_file(name: String) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export 4050 Flash Drive file")
        .set_file_name(name)
        .save_file()
}

pub(super) fn save_flash_archive() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Flash Drive archive")
        .set_file_name("Archive.zip")
        .add_filter("ZIP archive", &["zip"])
        .save_file()
}

pub(super) fn save_screen_copy() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Save screen copy")
        .set_file_name("screen.png")
        .add_filter("PNG image", &["png"])
        .save_file()
}
