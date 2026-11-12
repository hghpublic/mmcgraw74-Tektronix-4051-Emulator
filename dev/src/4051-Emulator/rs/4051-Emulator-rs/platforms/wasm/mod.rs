//! Browser-facing bindings for the reusable, UI-independent emulator session.
//!
//! The browser owns DOM event lifecycle and selects either CPU pixels or an
//! optional WebGPU surface for presentation. Shared Rust mapping rules translate
//! logical browser keys; this adapter advances the session from a browser-
//! compatible clock and returns snapshots without host file effects.

use std::{io::Cursor, path::Path, sync::Arc};

#[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
mod webgpu;
#[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
use webgpu::BrowserWebGpuPresentation;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use wasm_bindgen::prelude::*;

use crate::{
    emulation::{
        Command, CpuReferenceRenderer, DisplayGranularity, EmulationState, FileOutcome,
        SPEED_FACTORS, Snapshot,
        input::{AutoLoadState, InputAction, InputFrame, KeyModifiers, ManualKeyId},
        keyboard::{
            KeyMapping, KeyboardKey, map_host_key, map_text_character,
            map_text_character_with_host_shift,
        },
    },
    flash_drive::{
        FileType, FileUsage,
        archive::{ArchiveFileRef, OwnedArchiveFile, write_archive},
    },
    flash_drive_import::{
        FlashArchiveBudget, FlashImportOrder, FlashImportSelection, read_flash_archive_bytes,
    },
    machine::{
        Machine,
        gpib::{FlashDriveFile, FlashDriveFileInfo, MAX_FLASH_ARCHIVE_FILE_BYTES},
    },
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Display modes understood by the browser API.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserGranularity {
    Fine,
    Coarse,
    Mixed,
}

/// Auto Load state published by the shared input sequencer.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAutoLoadState {
    Idle,
    Typing,
    Sent,
    Cancelled,
}

/// Returns the maximum compressed size accepted for one selected Flash Drive ZIP.
#[wasm_bindgen]
pub fn max_flash_archive_file_bytes() -> u32 {
    MAX_FLASH_ARCHIVE_FILE_BYTES as u32
}

impl From<AutoLoadState> for BrowserAutoLoadState {
    fn from(state: AutoLoadState) -> Self {
        match state {
            AutoLoadState::Idle => Self::Idle,
            AutoLoadState::Typing => Self::Typing,
            AutoLoadState::Sent => Self::Sent,
            AutoLoadState::Cancelled => Self::Cancelled,
        }
    }
}

/// A browser-facing result from the shared keyboard mapping.
#[wasm_bindgen]
pub struct BrowserKeyMapping {
    code: u8,
    invert_shift: bool,
    text_input: bool,
}

#[wasm_bindgen]
impl BrowserKeyMapping {
    /// Returns the translated Tektronix keyboard code.
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> u8 {
        self.code
    }

    /// Returns whether the translated character reverses the Shift line.
    #[wasm_bindgen(getter)]
    pub fn invert_shift(&self) -> bool {
        self.invert_shift
    }

    /// Returns whether paired browser text should override the logical key.
    #[wasm_bindgen(getter)]
    pub fn text_input(&self) -> bool {
        self.text_input
    }
}

/// Maps `KeyboardEvent.key` and its optional paired text through the shared Rust rules.
///
/// The `key` argument uses browser `KeyboardEvent.key` values. If `text` is supplied
/// for a printable key, it must contain exactly one printable ASCII character.
#[wasm_bindgen]
pub fn map_browser_key(key: &str, text: Option<String>) -> Option<BrowserKeyMapping> {
    map_browser_key_inner(key, text, None)
}

/// Maps a browser key and optional paired text using its event-time Shift state.
///
/// Unlike [`map_browser_key`], this adjusts the shared printable mapping when
/// the host layout uses Shift differently from a conventional US keyboard.
#[wasm_bindgen]
pub fn map_browser_key_with_shift(
    key: &str,
    text: Option<String>,
    host_shift: bool,
) -> Option<BrowserKeyMapping> {
    map_browser_key_inner(key, text, Some(host_shift))
}

fn map_browser_key_inner(
    key: &str,
    text: Option<String>,
    host_shift: Option<bool>,
) -> Option<BrowserKeyMapping> {
    let host_key = browser_keyboard_key(key)?;
    let mapping = map_host_key(host_key)?;
    let mapping = if mapping.text_input {
        match text {
            Some(text) => {
                let character = single_ascii_character(&text)?;
                match host_shift {
                    Some(host_shift) => map_text_character_with_host_shift(character, host_shift)?,
                    None => map_text_character(character)?,
                }
            }
            None => match (host_key, host_shift) {
                (KeyboardKey::Printable(character), Some(host_shift)) => {
                    map_text_character_with_host_shift(character, host_shift)?
                }
                _ => mapping,
            },
        }
    } else {
        mapping
    };

    Some(BrowserKeyMapping::from(mapping))
}

impl From<KeyMapping> for BrowserKeyMapping {
    fn from(mapping: KeyMapping) -> Self {
        Self {
            code: mapping.code,
            invert_shift: mapping.invert_shift,
            text_input: mapping.text_input,
        }
    }
}

/// A batch of user-selected loose files or ZIP files in selection order.
#[wasm_bindgen]
#[derive(Default)]
pub struct BrowserImportSelection {
    items: Vec<BrowserImportItem>,
    prepared: Option<FlashImportSelection>,
    budget: FlashArchiveBudget,
}

enum BrowserImportItem {
    File {
        file: FlashDriveFile,
        decode_archive: bool,
    },
    Error(String),
    Prepared(FlashImportSelection),
}

#[wasm_bindgen]
impl BrowserImportSelection {
    /// Creates an empty selected-file batch.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one selected file; a `.zip` name is decoded with the shared bounded ZIP reader.
    pub fn add_file(&mut self, name: String, bytes: Vec<u8>) {
        self.push_item(BrowserImportItem::File {
            file: FlashDriveFile { name, bytes },
            decode_archive: true,
        });
    }

    /// Adds a host-side read or size error at this file's selection position.
    pub fn add_error(&mut self, name: String, message: String) {
        self.push_item(BrowserImportItem::Error(format!(
            "Import failed for {name}: {message}"
        )));
    }

    /// Adds a host-formatted failure without changing its selection order.
    pub fn add_prepared_error(&mut self, message: String) {
        self.push_item(BrowserImportItem::Error(message));
    }

    /// Adds a file whose ZIP members were already decoded by the shared worker API.
    pub fn add_prepared_file(&mut self, name: String, bytes: Vec<u8>) {
        self.push_item(BrowserImportItem::File {
            file: FlashDriveFile { name, bytes },
            decode_archive: false,
        });
    }

    /// Decodes selected ZIPs with the shared bounds before the main emulator applies the batch.
    pub fn prepare_selected_files(&mut self) {
        if self.prepared.is_some() {
            return;
        }
        let mut prepared = FlashImportSelection::default();
        for item in std::mem::take(&mut self.items) {
            match item {
                BrowserImportItem::Error(error) => prepared.push_error(error),
                BrowserImportItem::Prepared(selection) => prepared.append(selection),
                BrowserImportItem::File {
                    file,
                    decode_archive: true,
                } if is_zip_filename(&file.name) => {
                    match read_flash_archive_bytes(&file.bytes, &mut self.budget) {
                        Ok(mut decoded) => {
                            for error in &mut decoded.errors {
                                *error = format!("Import failed for {} ({error})", file.name);
                            }
                            prepared.append(decoded);
                        }
                        Err(error) => {
                            prepared.push_error(format!("Import failed for {}: {error}", file.name))
                        }
                    }
                }
                BrowserImportItem::File { file, .. } => prepared.push_file(file),
            }
        }
        self.prepared = Some(prepared);
    }

    /// Returns whether one prepared result is an error.
    pub fn result_is_error(&self, index: u32) -> bool {
        self.prepared
            .as_ref()
            .and_then(|selection| selection.order.get(index as usize))
            .is_some_and(|item| matches!(item, FlashImportOrder::Error(_)))
    }

    /// Returns one prepared error in original selection order.
    pub fn result_error(&self, index: u32) -> Option<String> {
        let selection = self.prepared.as_ref()?;
        let FlashImportOrder::Error(error_index) = *selection.order.get(index as usize)? else {
            return None;
        };
        selection.errors.get(error_index).cloned()
    }

    /// Returns one prepared file in original selection order.
    pub fn result_file(&self, index: u32) -> Option<BrowserExportFile> {
        let selection = self.prepared.as_ref()?;
        let FlashImportOrder::File(file_index) = *selection.order.get(index as usize)? else {
            return None;
        };
        let file = selection.files.get(file_index)?;
        Some(BrowserExportFile {
            name: file.name.clone(),
            bytes: file.bytes.clone(),
        })
    }

    /// Returns the number of selected files accumulated in this batch.
    pub fn len(&self) -> u32 {
        self.prepared.as_ref().map_or_else(
            || {
                self.items
                    .iter()
                    .map(|item| match item {
                        BrowserImportItem::Prepared(selection) => {
                            selection.files.len() + selection.errors.len()
                        }
                        BrowserImportItem::Error(_) | BrowserImportItem::File { .. } => 1,
                    })
                    .sum()
            },
            |selection| selection.files.len() + selection.errors.len(),
        ) as u32
    }

    /// Returns whether the batch contains no selected files.
    pub fn is_empty(&self) -> bool {
        self.prepared.as_ref().map_or_else(
            || {
                self.items.iter().all(|item| match item {
                    BrowserImportItem::Prepared(selection) => {
                        selection.files.is_empty() && selection.errors.is_empty()
                    }
                    BrowserImportItem::Error(_) | BrowserImportItem::File { .. } => false,
                })
            },
            |selection| selection.files.is_empty() && selection.errors.is_empty(),
        )
    }
}

impl BrowserImportSelection {
    fn push_item(&mut self, item: BrowserImportItem) {
        if let Some(prepared) = self.prepared.take() {
            self.items.push(BrowserImportItem::Prepared(prepared));
        }
        self.items.push(item);
    }

    fn take_prepared(&mut self) -> Result<FlashImportSelection, String> {
        self.prepare_selected_files();
        let selection = self
            .prepared
            .take()
            .ok_or_else(|| "Flash Drive import preparation failed".to_owned())?;
        selection.validate_order()?;
        Ok(selection)
    }
}

/// A numbered Flash Drive export with its canonical filename and exact bytes.
#[wasm_bindgen]
pub struct BrowserExportFile {
    name: String,
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl BrowserExportFile {
    /// Returns the canonical Flash Drive filename.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Copies the exact stored file bytes to a JavaScript `Uint8Array`.
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

/// Raw numbered-file metadata and bytes for lossless archive encoding.
#[wasm_bindgen]
pub struct BrowserArchiveFile {
    number: u8,
    file_type_code: Option<u8>,
    usage_code: Option<u8>,
    description: Vec<u16>,
    bytes: Vec<u8>,
}

impl From<OwnedArchiveFile> for BrowserArchiveFile {
    fn from(file: OwnedArchiveFile) -> Self {
        Self {
            number: file.number,
            file_type_code: file.file_type.map(FileType::code),
            usage_code: file.usage.map(FileUsage::code),
            description: file.description,
            bytes: file.bytes,
        }
    }
}

#[wasm_bindgen]
impl BrowserArchiveFile {
    /// Returns the stored file number.
    #[wasm_bindgen(getter)]
    pub fn number(&self) -> u8 {
        self.number
    }

    /// Returns the raw file-type byte, if the store contains one.
    #[wasm_bindgen(getter)]
    pub fn file_type_code(&self) -> Option<u8> {
        self.file_type_code
    }

    /// Returns the raw file-usage byte, if the store contains one.
    #[wasm_bindgen(getter)]
    pub fn usage_code(&self) -> Option<u8> {
        self.usage_code
    }

    /// Copies the exact UTF-16 description units, including padding.
    pub fn description_utf16(&self) -> Vec<u16> {
        self.description.clone()
    }

    /// Copies the exact stored bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Returns the stored byte length.
    #[wasm_bindgen(getter)]
    pub fn byte_len(&self) -> u32 {
        self.bytes.len() as u32
    }

    /// Copies a bounded byte range for responsive transfer to a worker.
    pub fn bytes_range(&self, start: u32, length: u32) -> Result<Vec<u8>, String> {
        let start = start as usize;
        let end = start
            .checked_add(length as usize)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| "archive snapshot byte range is out of bounds".to_owned())?;
        Ok(self.bytes[start..end].to_vec())
    }
}

/// Stateless shared-code ZIP encoder used by the browser worker.
#[wasm_bindgen]
#[derive(Default)]
pub struct BrowserArchiveWriter {
    files: Vec<OwnedArchiveFile>,
}

#[wasm_bindgen]
impl BrowserArchiveWriter {
    /// Creates an empty archive writer.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one raw store snapshot entry without interpreting its description.
    pub fn add_file(
        &mut self,
        number: u8,
        file_type_code: Option<u8>,
        usage_code: Option<u8>,
        description: Vec<u16>,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let file_type = file_type_code
            .map(|code| {
                FileType::from_byte(code)
                    .ok_or_else(|| format!("unknown Flash Drive file type byte {code}"))
            })
            .transpose()?;
        let usage = usage_code
            .map(|code| {
                FileUsage::from_byte(code)
                    .ok_or_else(|| format!("unknown Flash Drive usage byte {code}"))
            })
            .transpose()?;
        self.files.push(OwnedArchiveFile {
            number,
            file_type,
            usage,
            description,
            bytes,
        });
        Ok(())
    }

    /// Encodes the snapshot with the shared archive writer.
    pub fn finish(&self) -> Result<Vec<u8>, String> {
        let files = self.files.iter().map(|file| ArchiveFileRef {
            number: file.number,
            file_type: file.file_type,
            usage: file.usage,
            description: &file.description,
            bytes: &file.bytes,
        });
        write_archive(Cursor::new(Vec::new()), files).map(Cursor::into_inner)
    }
}

/// Lossless directory metadata for one numbered Flash Drive file.
#[wasm_bindgen]
pub struct BrowserDirectoryEntry {
    number: u8,
    file_type: Option<String>,
    usage: Option<String>,
    description: Vec<u16>,
    byte_len: u64,
}

#[wasm_bindgen]
impl BrowserDirectoryEntry {
    /// Returns the file number.
    #[wasm_bindgen(getter)]
    pub fn number(&self) -> u8 {
        self.number
    }

    /// Returns the canonical type label, if present.
    #[wasm_bindgen(getter)]
    pub fn file_type(&self) -> Option<String> {
        self.file_type.clone()
    }

    /// Returns the canonical usage label, if present.
    #[wasm_bindgen(getter)]
    pub fn usage(&self) -> Option<String> {
        self.usage.clone()
    }

    /// Copies the exact UTF-16 description units, including unpaired surrogates and padding.
    pub fn description_utf16(&self) -> Vec<u16> {
        self.description.clone()
    }

    /// Returns the stored byte length.
    #[wasm_bindgen(getter)]
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

impl From<FlashDriveFileInfo> for BrowserDirectoryEntry {
    fn from(file: FlashDriveFileInfo) -> Self {
        Self {
            number: file.number,
            file_type: file.file_type.map(str::to_owned),
            usage: file.usage.map(str::to_owned),
            description: file.description,
            byte_len: file.byte_len as u64,
        }
    }
}

/// An operation notice published by the shared session.
#[wasm_bindgen]
pub struct BrowserNotice {
    error: bool,
    message: String,
}

#[wasm_bindgen]
impl BrowserNotice {
    /// Returns whether the notice represents an error.
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> bool {
        self.error
    }

    /// Returns the user-visible notice text.
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

impl From<(bool, String)> for BrowserNotice {
    fn from((error, message): (bool, String)) -> Self {
        Self { error, message }
    }
}

fn browser_keyboard_key(key: &str) -> Option<KeyboardKey> {
    Some(match key {
        "Backspace" => KeyboardKey::Backspace,
        "Tab" => KeyboardKey::Tab,
        "Enter" => KeyboardKey::Enter,
        "Escape" => KeyboardKey::Escape,
        "Delete" => KeyboardKey::Delete,
        "Insert" => KeyboardKey::Insert,
        "Home" => KeyboardKey::Home,
        "End" => KeyboardKey::End,
        "PageUp" => KeyboardKey::PageUp,
        _ => {
            if let Some(function) = key
                .strip_prefix('F')
                .and_then(|number| number.parse::<u8>().ok())
                .filter(|number| (1..=35).contains(number))
            {
                if key != format!("F{function}") {
                    return None;
                }
                KeyboardKey::Function(function)
            } else {
                KeyboardKey::Printable(single_ascii_character(key)?)
            }
        }
    })
}

fn single_ascii_character(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let character = characters.next()?;
    (character.is_ascii() && characters.next().is_none()).then_some(character)
}

fn is_zip_filename(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

/// A session controlled by a browser event loop.
///
/// Call [`advance`](BrowserEmulator::advance) from the browser's animation loop.
/// Use [`map_browser_key`] to map logical browser keys before calling `key_down`;
/// event pairing, key ownership, modifiers, focus and rendering remain host tasks.
#[wasm_bindgen]
pub struct BrowserEmulator {
    session: EmulationState,
    archive_export_snapshot: Option<Vec<u8>>,
    archive_export_was_modal: Option<bool>,
    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    webgpu_presentation: Option<BrowserWebGpuPresentation>,
}

#[wasm_bindgen]
impl BrowserEmulator {
    /// Resets the default ROM-backed machine and starts a paused session.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<BrowserEmulator, String> {
        let mut machine = Machine::default();
        machine
            .reset()
            .map_err(|error| format!("machine reset failed: {error}"))?;
        Ok(Self {
            session: EmulationState::new(machine),
            archive_export_snapshot: None,
            archive_export_was_modal: None,
            #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
            webgpu_presentation: None,
        })
    }

    /// Tries to install browser WebGPU presentation, retaining CPU rendering on failure.
    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub async fn initialize_webgpu(&mut self, canvas: web_sys::HtmlCanvasElement) -> bool {
        let initialized = async {
            let (renderer, presentation) = BrowserWebGpuPresentation::initialize(
                canvas,
                CpuReferenceRenderer::WIDTH as u32,
                CpuReferenceRenderer::HEIGHT as u32,
            )
            .await?;
            self.session.enable_webgpu_renderer(renderer)?;
            self.webgpu_presentation = Some(presentation);
            Ok::<(), String>(())
        }
        .await;

        match initialized {
            Ok(()) => true,
            Err(error) => {
                self.session.record_webgpu_error(&error);
                false
            }
        }
    }

    /// Tries to install the opt-in phosphor-aging WebGPU renderer.
    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub async fn initialize_extended_webgpu(&mut self, canvas: web_sys::HtmlCanvasElement) -> bool {
        let initialized = async {
            let (renderer, presentation) = BrowserWebGpuPresentation::initialize_extended(
                canvas,
                CpuReferenceRenderer::WIDTH as u32,
                CpuReferenceRenderer::HEIGHT as u32,
            )
            .await?;
            self.session.enable_extended_webgpu_renderer(renderer)?;
            self.webgpu_presentation = Some(presentation);
            Ok::<(), String>(())
        }
        .await;

        match initialized {
            Ok(()) => true,
            Err(error) => {
                self.session.record_webgpu_error(&error);
                false
            }
        }
    }

    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    fn sync_webgpu_presentation_source(&mut self, required: bool) -> Result<(), String> {
        let Some(source) = self.session.vector_zoom_presentation_view() else {
            return if required {
                Err("Extended WebGPU presentation is not active".to_owned())
            } else {
                Ok(())
            };
        };
        let presentation = self
            .webgpu_presentation
            .as_mut()
            .ok_or_else(|| "WebGPU presentation is not active".to_owned())?;
        presentation.set_source_view(source);
        Ok(())
    }

    /// Presents the current WebGPU frame; a surface failure switches to CPU pixels.
    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub fn present_webgpu(&mut self) -> Result<(), String> {
        let result = self
            .webgpu_presentation
            .as_ref()
            .map(BrowserWebGpuPresentation::present)
            .transpose();
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                self.webgpu_presentation = None;
                self.session.record_webgpu_failure(&error);
                Err(error)
            }
        }
    }

    /// Advances the session to the current browser-compatible monotonic time.
    ///
    /// The returned snapshot retains its display pixels until it is dropped.
    pub fn advance(&mut self) -> BrowserSnapshot {
        self.advance_time();
        self.snapshot()
    }

    /// Advances to now before changing whether CPU execution is running.
    pub fn set_running(&mut self, running: bool) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::Run(running))
    }

    /// Resets the machine while preserving the session's current run state.
    pub fn reset(&mut self) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::Reset)
    }

    /// Enables optional vector-scene capture at an existing-reset hard cut.
    ///
    /// Enabling requires Extended WebGPU and resets the machine before capture
    /// continues. Disabling releases retained scene data without resetting it.
    #[cfg(feature = "webgpu")]
    pub fn set_vector_zoom_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if enabled == self.session.vector_scene_capture_enabled() {
            return Ok(());
        }

        if !enabled {
            self.session.set_vector_scene_capture_enabled(false)?;
            #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
            self.sync_webgpu_presentation_source(false)?;
            return Ok(());
        }

        self.session.set_vector_scene_capture_enabled(true)?;
        if let Err(error) = self.reset() {
            return match self.session.set_vector_scene_capture_enabled(false) {
                Ok(()) => Err(error),
                Err(cleanup_error) => {
                    Err(format!("{error}; capture cleanup failed: {cleanup_error}"))
                }
            };
        }

        let snapshot = self.snapshot();
        if let Some(error) = snapshot.runtime_error().or_else(|| snapshot.render_error()) {
            return match self.session.set_vector_scene_capture_enabled(false) {
                Ok(()) => Err(error),
                Err(cleanup_error) => {
                    Err(format!("{error}; capture cleanup failed: {cleanup_error}"))
                }
            };
        }

        #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
        self.sync_webgpu_presentation_source(true)?;
        Ok(())
    }

    /// Selects the vector scene as a presentation-only view while capture is enabled.
    #[cfg(feature = "webgpu")]
    pub fn set_vector_zoom_mode(&mut self, enabled: bool) -> Result<(), String> {
        self.session.set_vector_zoom_mode(enabled)?;
        #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
        self.sync_webgpu_presentation_source(true)?;
        Ok(())
    }

    /// Updates the vector presentation camera in logical display coordinates without advancing emulation.
    #[cfg(feature = "webgpu")]
    pub fn set_vector_zoom_camera(
        &mut self,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
    ) -> Result<(), String> {
        self.session.set_vector_zoom_camera(zoom, pan_x, pan_y)
    }

    /// Selects one of the supported CPU speed factors.
    pub fn set_speed(&mut self, index: u32) -> Result<(), String> {
        if SPEED_FACTORS.get(index as usize).is_none() {
            return Err(format!("invalid speed index: {index}"));
        }
        self.advance_time();
        self.apply_without_file_effect(Command::Speed(index as usize))
    }

    /// Changes the display's fine, coarse, or mixed pixel mode.
    pub fn set_display_granularity(
        &mut self,
        granularity: BrowserGranularity,
    ) -> Result<(), String> {
        self.advance_time();
        let granularity = match granularity {
            BrowserGranularity::Fine => DisplayGranularity::Fine,
            BrowserGranularity::Coarse => DisplayGranularity::Coarse,
            BrowserGranularity::Mixed => DisplayGranularity::Mixed,
        };
        self.apply_without_file_effect(Command::Granularity(granularity))
    }

    /// Sets the session's Caps Lock state.
    pub fn set_caps_lock(&mut self, enabled: bool) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::CapsLock(enabled))
    }

    /// Clears readiness or records legacy request metadata after cursor detection.
    ///
    /// Setting `ready` to `true` cannot create readiness; the shared core must
    /// already have detected a BASIC cursor or transient INPUT-prompt glyph.
    pub fn set_basic_prompt_ready(&mut self, ready: bool, request: u64) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::PromptReady { ready, request })
    }

    /// Starts Auto Load when the running session has detected a BASIC prompt.
    pub fn auto_load(&mut self) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::AutoLoad)?;
        self.session
            .snapshot()
            .auto_load_error
            .clone()
            .map_or(Ok(()), Err)
    }

    /// Queues a numbered ASCII or BINARY PROG only while the machine runs and BASIC is ready.
    pub fn launch_file(&mut self, number: u8) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::Launch(number))?;
        match self.session.snapshot().program_launch_notice.clone() {
            Some((true, message)) => Err(message),
            _ => Ok(()),
        }
    }

    /// Queues numbered BASIC source lines, waits for each prompt, and runs the program.
    pub fn queue_basic_program(&mut self, lines: Vec<String>) -> Result<(), String> {
        self.advance_time();
        self.session.queue_basic_program(lines)
    }

    /// Imports a user-selected loose-file/ZIP batch through the shared bounded ZIP decoder.
    ///
    /// The selection is consumed. Its file order and per-file failures are published in snapshots.
    pub fn import_selected_files(
        &mut self,
        selection: &mut BrowserImportSelection,
    ) -> Result<(), String> {
        self.ensure_archive_snapshot_inactive()?;
        self.advance_time();
        let was_modal = self.session.begin_file_io();
        let result = selection
            .take_prepared()
            .and_then(|selected| self.session.import_ordered_files(selected));
        self.session.finish_file_io(Instant::now(), was_modal);
        result
    }

    /// Transactionally replaces the whole Flash Drive from a user-confirmed archive.
    pub fn replace_flash_drive_archive(
        &mut self,
        confirmed: bool,
        bytes: Vec<u8>,
    ) -> Result<Option<u32>, String> {
        if !confirmed {
            return Ok(None);
        }
        self.ensure_archive_snapshot_inactive()?;
        self.advance_time();
        let was_modal = self.session.begin_file_io();
        let result = self
            .session
            .import_archive(Cursor::new(bytes))
            .map(|count| count as u32);
        self.session.finish_file_io(Instant::now(), was_modal);
        match result {
            Ok(count) => {
                self.session.complete_file_effect(FileOutcome::Import((
                    false,
                    format!("Flash Drive storage replaced from archive ({count} file(s))"),
                )));
                Ok(Some(count))
            }
            Err(error) => {
                let message = format!("Flash Drive archive import failed: {error}");
                self.session
                    .complete_file_effect(FileOutcome::Import((true, message.clone())));
                Err(message)
            }
        }
    }

    /// Clears the in-memory Flash Drive store unless an archive snapshot is active.
    pub fn clear_flash_drive(&mut self) -> Result<(), String> {
        self.ensure_archive_snapshot_inactive()?;
        self.advance_time();
        let was_modal = self.session.begin_file_io();
        self.session.clear_flash_drive();
        self.session.finish_file_io(Instant::now(), was_modal);
        Ok(())
    }

    /// Returns one canonical numbered file for a browser download.
    pub fn export_file(&mut self, number: u8) -> Result<BrowserExportFile, String> {
        self.advance_time();
        match self.session.export_file(number) {
            Ok(file) => {
                self.session.complete_file_effect(FileOutcome::Export((
                    false,
                    format!("Flash Drive file {number} exported: {}", file.name),
                )));
                Ok(BrowserExportFile {
                    name: file.name,
                    bytes: file.bytes,
                })
            }
            Err(error) => {
                let message = format!("Flash Drive export failed: {error}");
                self.session
                    .complete_file_effect(FileOutcome::Export((true, message.clone())));
                Err(message)
            }
        }
    }

    /// Encodes the current whole-store archive for a browser download.
    pub fn export_flash_drive_archive(&mut self) -> Result<Vec<u8>, String> {
        self.advance_time();
        let was_modal = self.session.begin_file_io();
        let result = self
            .session
            .write_archive(Cursor::new(Vec::new()))
            .map(Cursor::into_inner);
        self.session.finish_file_io(Instant::now(), was_modal);
        match result {
            Ok(bytes) => {
                self.session.complete_file_effect(FileOutcome::Export((
                    false,
                    "Flash Drive archive exported".to_owned(),
                )));
                Ok(bytes)
            }
            Err(error) => {
                let message = format!("Flash Drive archive export failed: {error}");
                self.session
                    .complete_file_effect(FileOutcome::Export((true, message.clone())));
                Err(message)
            }
        }
    }

    /// Starts a stable, lossless store snapshot for the browser archive worker.
    pub fn begin_archive_export_snapshot(&mut self) -> Result<u32, String> {
        if self.archive_export_snapshot.is_some() {
            return Err("a Flash Drive archive snapshot is already active".to_owned());
        }
        self.advance_time();
        let was_modal = self.session.begin_file_io();
        let numbers = self
            .session
            .snapshot()
            .directory
            .iter()
            .map(|entry| entry.number)
            .collect::<Vec<_>>();
        let count = numbers.len() as u32;
        self.archive_export_was_modal = Some(was_modal);
        self.archive_export_snapshot = Some(numbers);
        Ok(count)
    }

    /// Transfers one file from the active stable snapshot to a worker.
    pub fn archive_export_snapshot_file(&mut self, index: u32) -> Option<BrowserArchiveFile> {
        let number = *self.archive_export_snapshot.as_ref()?.get(index as usize)?;
        self.session
            .flash_drive_archive_file(number)
            .map(BrowserArchiveFile::from)
    }

    /// Releases the snapshot and restores the session's prior modal state.
    pub fn finish_archive_export_snapshot(&mut self) {
        self.archive_export_snapshot = None;
        if let Some(was_modal) = self.archive_export_was_modal.take() {
            self.session.finish_file_io(Instant::now(), was_modal);
        }
    }

    /// Publishes the worker's archive result through the shared session notice.
    pub fn publish_archive_export_notice(&mut self, error: bool, message: String) {
        self.session
            .complete_file_effect(FileOutcome::Export((error, message)));
    }

    /// Queues a browser key press so short DOM taps reach the emulated keyboard.
    pub fn key_down(
        &mut self,
        key_id: u32,
        tek_code: u8,
        invert_shift: bool,
        shift: bool,
        control: bool,
    ) -> Result<(), String> {
        let modifiers = KeyModifiers { shift, control };
        self.apply_input(InputFrame {
            actions: vec![InputAction::SequencedManualPress {
                key: ManualKeyId(u64::from(key_id)),
                code: tek_code,
                invert_shift,
                modifiers,
            }],
            modifiers,
            focused: true,
            reset_input: false,
        })
    }

    /// Releases the matching browser key after its minimum press interval.
    pub fn key_up(&mut self, key_id: u32, shift: bool, control: bool) -> Result<(), String> {
        let modifiers = KeyModifiers { shift, control };
        self.apply_input(InputFrame {
            actions: vec![InputAction::SequencedManualRelease(ManualKeyId(u64::from(
                key_id,
            )))],
            modifiers,
            focused: true,
            reset_input: false,
        })
    }

    /// Returns queued key transitions that have not yet reached the emulated keyboard.
    pub fn pending_input_transitions(&self) -> usize {
        self.session.pending_input_transitions()
    }

    /// Applies modifier changes that are not paired with a key press or release.
    pub fn set_modifiers(&mut self, shift: bool, control: bool) -> Result<(), String> {
        let modifiers = KeyModifiers { shift, control };
        self.apply_input(InputFrame {
            actions: vec![InputAction::Modifiers(modifiers)],
            modifiers,
            focused: true,
            reset_input: false,
        })
    }

    /// Releases held keys and modifiers when the browser surface loses focus.
    pub fn release_input(&mut self) -> Result<(), String> {
        self.apply_input(InputFrame::unfocused())
    }

    /// Reports whether a manual or front-panel key is currently held.
    pub fn has_held_key(&self) -> bool {
        self.session.has_held_key()
    }

    /// Applies a front-panel key press through the session's timed input sequence.
    pub fn panel_press(&mut self, tek_code: u8, shift: bool, control: bool) -> Result<(), String> {
        let modifiers = KeyModifiers { shift, control };
        self.apply_input(InputFrame {
            actions: vec![InputAction::PanelPress {
                code: tek_code,
                modifiers,
            }],
            modifiers,
            focused: true,
            reset_input: false,
        })
    }

    /// Releases a front-panel key while retaining the current modifiers.
    pub fn panel_release(&mut self, shift: bool, control: bool) -> Result<(), String> {
        let modifiers = KeyModifiers { shift, control };
        self.apply_input(InputFrame {
            actions: vec![InputAction::PanelRelease(modifiers)],
            modifiers,
            focused: true,
            reset_input: false,
        })
    }

    /// Publishes the current immutable session state without advancing time.
    pub fn snapshot(&mut self) -> BrowserSnapshot {
        BrowserSnapshot {
            snapshot: self.session.snapshot(),
        }
    }

    fn advance_time(&mut self) {
        self.session.advance_realtime(Instant::now());
    }

    fn apply_input(&mut self, frame: InputFrame) -> Result<(), String> {
        self.advance_time();
        self.apply_without_file_effect(Command::Input(Box::new(frame)))
    }

    fn apply_without_file_effect(&mut self, command: Command) -> Result<(), String> {
        if self.archive_export_snapshot.is_some() && !matches!(&command, Command::Input(_)) {
            return Err(
                "emulator commands are suspended during a Flash Drive archive snapshot".to_owned(),
            );
        }
        match self.session.apply(command)? {
            None => Ok(()),
            Some(_) => Err("browser API command requested an unsupported host file effect".into()),
        }
    }

    fn ensure_archive_snapshot_inactive(&self) -> Result<(), String> {
        if self.archive_export_snapshot.is_some() {
            return Err("Flash Drive storage is frozen during archive export".to_owned());
        }
        Ok(())
    }
}

/// An immutable snapshot returned to JavaScript.
///
/// Pixel values are row-major green-channel intensities. Calling [`pixels`](Self::pixels)
/// copies the current frame to a JavaScript `Uint8Array`; use `revision` to avoid
/// requesting a copy when the display has not changed.
#[wasm_bindgen]
pub struct BrowserSnapshot {
    snapshot: Arc<Snapshot>,
}

#[wasm_bindgen]
impl BrowserSnapshot {
    /// Returns the logical screen width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        CpuReferenceRenderer::WIDTH as u32
    }

    /// Returns the logical screen height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        CpuReferenceRenderer::HEIGHT as u32
    }

    /// Returns the display revision as a JavaScript BigInt.
    #[wasm_bindgen(getter)]
    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }

    /// Copies the row-major green-channel frame into a JavaScript `Uint8Array`.
    pub fn pixels(&self) -> Vec<u8> {
        self.snapshot.pixels.to_vec()
    }

    /// Returns whether browser frames are presented by WebGPU.
    #[wasm_bindgen(getter)]
    pub fn webgpu_active(&self) -> bool {
        self.snapshot.webgpu_active
    }

    /// Returns whether frames are presented by the Extended WebGPU renderer.
    #[wasm_bindgen(getter)]
    pub fn extended_webgpu_active(&self) -> bool {
        self.snapshot.extended_webgpu_active
    }

    /// Returns the reason the CPU reference renderer is active after a WebGPU fallback.
    #[wasm_bindgen(getter)]
    pub fn render_error(&self) -> Option<String> {
        self.snapshot.render_error.clone()
    }

    /// Returns whether CPU execution is active.
    #[wasm_bindgen(getter)]
    pub fn running(&self) -> bool {
        self.snapshot.running
    }

    /// Returns the selected index in `emulation::SPEED_FACTORS`.
    #[wasm_bindgen(getter)]
    pub fn speed_index(&self) -> u32 {
        self.snapshot.speed_index as u32
    }

    /// Returns the current Caps Lock state.
    #[wasm_bindgen(getter)]
    pub fn caps_lock(&self) -> bool {
        self.snapshot.caps_lock
    }

    /// Returns a runtime failure, if execution stopped on a machine error.
    #[wasm_bindgen(getter)]
    pub fn runtime_error(&self) -> Option<String> {
        self.snapshot.runtime_error.clone()
    }

    /// Returns a speed-setting error, if one is present.
    #[wasm_bindgen(getter)]
    pub fn settings_error(&self) -> Option<String> {
        self.snapshot.settings_error.clone()
    }

    /// Returns the active fine, coarse, or mixed display mode.
    #[wasm_bindgen(getter)]
    pub fn display_granularity(&self) -> BrowserGranularity {
        match self.snapshot.granularity {
            DisplayGranularity::Fine => BrowserGranularity::Fine,
            DisplayGranularity::Coarse => BrowserGranularity::Coarse,
            DisplayGranularity::Mixed => BrowserGranularity::Mixed,
        }
    }

    /// Returns the MC6800 accumulator A.
    #[wasm_bindgen(getter)]
    pub fn register_a(&self) -> u8 {
        self.snapshot.registers.a
    }

    /// Returns the MC6800 accumulator B.
    #[wasm_bindgen(getter)]
    pub fn register_b(&self) -> u8 {
        self.snapshot.registers.b
    }

    /// Returns the MC6800 index register.
    #[wasm_bindgen(getter)]
    pub fn register_ix(&self) -> u16 {
        self.snapshot.registers.ix
    }

    /// Returns the MC6800 program counter.
    #[wasm_bindgen(getter)]
    pub fn register_pc(&self) -> u16 {
        self.snapshot.registers.pc
    }

    /// Returns the MC6800 stack pointer.
    #[wasm_bindgen(getter)]
    pub fn register_sp(&self) -> u16 {
        self.snapshot.registers.sp
    }

    /// Returns the MC6800 condition-code register.
    #[wasm_bindgen(getter)]
    pub fn register_ccr(&self) -> u8 {
        self.snapshot.registers.ccr
    }

    /// Returns whether the keyboard PIA asserts BUSY.
    #[wasm_bindgen(getter)]
    pub fn busy(&self) -> bool {
        self.snapshot.indicators.busy
    }

    /// Returns whether the keyboard PIA asserts I/O.
    #[wasm_bindgen(getter)]
    pub fn io(&self) -> bool {
        self.snapshot.indicators.io
    }

    /// Returns whether the keyboard PIA asserts BREAK.
    #[wasm_bindgen(getter)]
    pub fn break_active(&self) -> bool {
        self.snapshot.indicators.break_active
    }

    /// Returns whether the shared core has detected a BASIC cursor or INPUT prompt.
    #[wasm_bindgen(getter)]
    pub fn basic_prompt_ready(&self) -> bool {
        self.snapshot.basic_prompt_ready
    }

    /// Returns an explanation when keyboard presses could not be delivered while stopped.
    #[wasm_bindgen(getter)]
    pub fn input_notice(&self) -> Option<String> {
        self.snapshot.input_notice.clone()
    }

    /// Returns the request identifier from the latest legacy readiness update, or zero.
    #[wasm_bindgen(getter)]
    pub fn prompt_request(&self) -> u64 {
        self.snapshot.prompt_request
    }

    /// Returns the current Auto Load sequencing state.
    #[wasm_bindgen(getter)]
    pub fn auto_load_state(&self) -> BrowserAutoLoadState {
        self.snapshot.auto_load_state.into()
    }

    /// Returns an Auto Load interruption or validation error, if present.
    #[wasm_bindgen(getter)]
    pub fn auto_load_error(&self) -> Option<String> {
        self.snapshot.auto_load_error.clone()
    }

    /// Returns the selected program number, if a directory launch is pending or active.
    #[wasm_bindgen(getter)]
    pub fn program_launch_file(&self) -> Option<u8> {
        self.snapshot.program_launch_file
    }

    /// Returns the latest directory-launch notice, if present.
    #[wasm_bindgen(getter)]
    pub fn program_launch_notice(&self) -> Option<BrowserNotice> {
        self.snapshot
            .program_launch_notice
            .clone()
            .map(BrowserNotice::from)
    }

    /// Returns the number of canonical directory rows.
    #[wasm_bindgen(getter)]
    pub fn directory_len(&self) -> u32 {
        self.snapshot.directory.len() as u32
    }

    /// Returns one immutable directory row in canonical numeric order.
    pub fn directory_entry(&self, index: u32) -> Option<BrowserDirectoryEntry> {
        self.snapshot
            .directory
            .get(index as usize)
            .cloned()
            .map(BrowserDirectoryEntry::from)
    }

    /// Returns the number of ordered import notices.
    #[wasm_bindgen(getter)]
    pub fn import_notice_count(&self) -> u32 {
        self.snapshot.import_notices.len() as u32
    }

    /// Returns one ordered import notice.
    pub fn import_notice(&self, index: u32) -> Option<BrowserNotice> {
        self.snapshot
            .import_notices
            .get(index as usize)
            .cloned()
            .map(BrowserNotice::from)
    }

    /// Returns the latest export notice, if present.
    #[wasm_bindgen(getter)]
    pub fn export_notice(&self) -> Option<BrowserNotice> {
        self.snapshot.export_notice.clone().map(BrowserNotice::from)
    }

    /// Returns whether the shared session is suspended for a host file operation.
    #[wasm_bindgen(getter, js_name = file_io_busy)]
    pub fn file_io_busy(&self) -> bool {
        self.snapshot.busy
    }
}
