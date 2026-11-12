//! Coordinates mutable machine execution and publishes framework-neutral state.
//!
//! Commands preserve input, lifecycle and acknowledged file-operation ordering.
//! Immutable snapshots share grayscale pixels until the display revision changes;
//! retained screen copies keep their event-time pixels independently of that view.
//! Real-time advancement coalesces CPU work while retaining erase wall time.
//! File commands emit concrete effects rather than opening or encoding host files.
//! The host owner executes those effects before further commands or advancement,
//! then records their outcomes here. Threading and presentation belong to the host.
//! Owned data stays private; consumers use lifecycle operations and archive
//! streams. Native private-state fixtures are compiled only as descendant tests.

use std::{sync::Arc, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(test)]
use super::display::DisplayEvent;
use super::{
    Command, FileEffect, FileOutcome, SPEED_FACTORS, Snapshot,
    cursor_readiness::CursorReadinessDetector,
    input::{AutoLoadState, InputAction, InputFrame, InputSequence, KeyModifiers, KeyboardState},
    runtime::HostRuntime,
};
use crate::flash_drive_import::{FlashImportOrder, FlashImportSelection};
use crate::machine::{Machine, gpib::FlashDriveFile};

#[cfg(all(test, feature = "native"))]
#[path = "../native/app.rs"]
mod native_tests;
#[cfg(all(test, feature = "native"))]
#[path = "test_support.rs"]
pub(crate) mod test_support;

/// Maximum logical elapsed time delivered by one coalesced realtime advance.
pub const TICK: Duration = Duration::from_millis(10);
const MAX_SCREEN_COPIES: usize = 8;
const MAX_NOTICES: usize = 64;

fn frame_has_press(frame: &InputFrame) -> bool {
    frame.actions.iter().any(|action| {
        matches!(
            action,
            InputAction::ManualPress { .. }
                | InputAction::SequencedManualPress { .. }
                | InputAction::PanelPress { .. }
        )
    })
}

fn frame_has_return(frame: &InputFrame) -> bool {
    frame.actions.iter().any(|action| {
        matches!(
            action,
            InputAction::ManualPress { code: 0x0d, .. }
                | InputAction::SequencedManualPress { code: 0x0d, .. }
                | InputAction::PanelPress { code: 0x0d, .. }
        )
    })
}

/// A paused, privately owned machine session with commands and immutable publications.
///
/// Hosts serialize command application, file-effect execution/completion and time
/// advancement with nondecreasing timestamps. Advance through the current host
/// time before changing Run/Pause state, so a later resume cannot execute paused
/// wall time. No worker, window or filesystem executor is created here.
pub struct EmulationState {
    data: SessionData,
}

// No public API returns this storage; descendant tests retain private fixture access.
pub(super) struct SessionData {
    pub(super) machine: Machine,
    pub(super) runtime: HostRuntime,
    pub(super) keyboard: KeyboardState,
    pub(super) input_sequence: InputSequence,
    pub(super) caps_lock: bool,
    pub(super) running: bool,
    pub(super) speed_index: usize,
    pub(super) settings_error: Option<String>,
    pub(super) last_modifiers: KeyModifiers,
    pub(super) last_update: Instant,
    pub(super) runtime_error: Option<String>,
    pub(super) basic_prompt_ready: bool,
    pub(super) cursor_readiness: CursorReadinessDetector,
    pub(super) input_notice: Option<String>,
    prompt_request: u64,
    pub(super) program_launch_file: Option<u8>,
    pub(super) program_launch_notice: Option<(bool, String)>,
    pub(super) auto_load_error: Option<String>,
    pub(super) import_notices: Vec<(bool, String)>,
    pub(super) export_notice: Option<(bool, String)>,
    pub(super) screen_copy_notices: Vec<(bool, String)>,
    pub(super) modal: bool,
    file_operation: u64,
    screen_copy_requests: Vec<(u64, Arc<[u8]>)>,
    next_screen_copy: u64,
    dropped_screen_copies: usize,
    cached_pixels: Option<(u64, Arc<[u8]>)>,
    coalesced_time: Duration,
    #[cfg(test)]
    display_event_trace: Vec<DisplayEvent>,
}

impl EmulationState {
    /// Initialize timing for an already prepared machine and pause execution.
    ///
    /// This does not reset the machine or change its starting registers. Call
    /// `Machine::reset` first for ROM boot, or explicitly configure a custom program.
    pub fn new(mut machine: Machine) -> Self {
        let mut runtime = HostRuntime::start(&mut machine);
        runtime.pause();
        runtime.clear_display_events();
        Self {
            data: SessionData {
                machine,
                runtime,
                keyboard: KeyboardState::default(),
                input_sequence: InputSequence::default(),
                caps_lock: false,
                running: false,
                speed_index: 0,
                settings_error: None,
                last_modifiers: KeyModifiers::NONE,
                last_update: Instant::now(),
                runtime_error: None,
                basic_prompt_ready: false,
                cursor_readiness: CursorReadinessDetector::default(),
                input_notice: None,
                prompt_request: 0,
                program_launch_file: None,
                program_launch_notice: None,
                auto_load_error: None,
                import_notices: Vec::new(),
                export_notice: None,
                screen_copy_notices: Vec::new(),
                modal: false,
                file_operation: 0,
                screen_copy_requests: Vec::new(),
                next_screen_copy: 0,
                dropped_screen_copies: 0,
                cached_pixels: None,
                coalesced_time: Duration::ZERO,
                #[cfg(test)]
                display_event_trace: Vec::new(),
            },
        }
    }

    #[cfg(feature = "webgpu")]
    pub fn enable_webgpu_renderer(
        &mut self,
        renderer: super::webgpu::WebGpuVectorRenderer,
    ) -> Result<(), String> {
        let result = self
            .data
            .runtime
            .enable_webgpu_renderer(&mut self.data.machine, renderer);
        if let Err(error) = &result {
            self.data.runtime.record_webgpu_error(error);
            return result;
        }
        self.data.cached_pixels = None;
        result
    }

    #[cfg(feature = "webgpu")]
    pub fn enable_extended_webgpu_renderer(
        &mut self,
        renderer: super::extended_webgpu::ExtendedWebGpuRenderer,
    ) -> Result<(), String> {
        let result = self
            .data
            .runtime
            .enable_extended_webgpu_renderer(&mut self.data.machine, renderer);
        if let Err(error) = &result {
            self.data.runtime.record_webgpu_error(error);
            return result;
        }
        self.data.cached_pixels = None;
        result
    }

    #[cfg(feature = "webgpu")]
    pub fn set_vector_scene_capture_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.data.runtime.set_vector_scene_capture_enabled(enabled)
    }

    #[cfg(feature = "webgpu")]
    pub fn set_vector_zoom_mode(&mut self, enabled: bool) -> Result<(), String> {
        self.data.runtime.set_vector_zoom_mode(enabled)
    }

    #[cfg(feature = "webgpu")]
    pub fn set_vector_zoom_camera(
        &mut self,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
    ) -> Result<(), String> {
        self.data.runtime.set_vector_zoom_camera(zoom, pan_x, pan_y)
    }

    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub(crate) fn vector_zoom_presentation_view(&self) -> Option<&wgpu::TextureView> {
        self.data.runtime.vector_zoom_presentation_view()
    }

    #[cfg(feature = "webgpu")]
    pub fn vector_scene_capture_enabled(&self) -> bool {
        self.data.runtime.vector_scene_capture_enabled()
    }

    #[cfg(feature = "webgpu")]
    pub fn vector_scene_memory_usage(&self) -> Option<(usize, usize)> {
        self.data.runtime.vector_scene_memory_usage()
    }

    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub(crate) fn record_webgpu_error(&mut self, error: &str) {
        self.data.runtime.record_webgpu_error(error);
        self.data.cached_pixels = None;
    }

    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub(crate) fn record_webgpu_failure(&mut self, error: &str) {
        self.data.runtime.record_webgpu_failure(error);
        self.data.cached_pixels = None;
    }

    /// Advance deterministic elapsed time using modifiers accepted by this session.
    /// Hosts must use nondecreasing timestamps, including repeated timestamps;
    /// earlier timestamps are outside this protocol and can replay elapsed time.
    pub fn advance_to(&mut self, now: Instant) {
        self.advance_with_modifiers(now, self.data.last_modifiers);
    }

    /// Temporarily suspend interval execution; retain the prior modal state for completion.
    pub fn begin_file_io(&mut self) -> bool {
        let was_modal = self.data.modal;
        self.data.modal = true;
        was_modal
    }

    /// Account for all file time while suspended, then restore the prior modal state.
    pub fn finish_file_io(&mut self, now: Instant, was_modal: bool) {
        self.advance_to(now);
        self.data.modal = was_modal;
    }

    /// Fatal command failures retain the error and release input while pausing execution.
    pub fn fail(&mut self, error: String) {
        self.data.runtime_error = Some(error);
        self.stop();
    }

    /// Owner shutdown cleanup, distinct from Pause: release input and clear readiness.
    pub fn stop(&mut self) {
        self.data.runtime.pause();
        self.data.running = false;
        self.data.basic_prompt_ready = false;
        self.data.cursor_readiness.reset();
        self.data.input_notice = None;
        self.data.runtime.clear_display_events();
        self.release_input();
    }

    /// Returns key transitions held in the input sequencer but not yet applied.
    pub fn pending_input_transitions(&self) -> usize {
        self.data.input_sequence.pending_key_transitions()
    }

    pub(crate) fn queue_basic_program(&mut self, lines: Vec<String>) -> Result<(), String> {
        if !self.data.running {
            return Err("the emulator must be running to execute a BASIC program".to_owned());
        }
        if !self.data.basic_prompt_ready {
            return Err("wait for a detected BASIC prompt before executing a program".to_owned());
        }
        self.data
            .input_sequence
            .queue_basic_program(lines, &mut self.data.keyboard, &mut self.data.machine)
            .map_err(str::to_owned)?;
        self.data.cursor_readiness.reset();
        self.data.basic_prompt_ready = false;
        Ok(())
    }

    /// Apply the existing transactional archive protocol and return the committed file count.
    pub fn import_archive<R: std::io::Read + std::io::Seek>(
        &mut self,
        source: R,
    ) -> Result<usize, String> {
        self.data.machine.import_flash_storage_archive(source)?;
        Ok(self.data.machine.flash_drive_directory().len())
    }

    /// Clear the in-memory Flash Drive store and abort any transfer addressed to it.
    pub fn clear_flash_drive(&mut self) {
        self.data.machine.clear_flash_drive();
    }

    /// Stream the suspended store without exposing its mutable machine or copying payloads.
    pub fn write_archive<W: std::io::Write + std::io::Seek>(
        &self,
        destination: W,
    ) -> Result<W, String> {
        self.data.machine.write_flash_storage_archive(destination)
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn flash_drive_archive_file(
        &self,
        number: u8,
    ) -> Option<crate::flash_drive::archive::OwnedArchiveFile> {
        self.data.machine.flash_drive_archive_file(number)
    }

    /// Return the canonical exported name and exact bytes for one stored file.
    pub fn export_file(&self, number: u8) -> Result<FlashDriveFile, String> {
        self.data
            .machine
            .export_flash_file(number)
            .map_err(|error| error.to_string())
    }

    /// Import prepared files in order and publish their per-file errors as session notices.
    pub fn import_files(&mut self, files: Vec<FlashDriveFile>, errors: Vec<String>) {
        self.apply_import_files(files, errors);
    }

    #[cfg(test)]
    pub(crate) fn take_vector_event_trace(&mut self) -> Vec<(i32, i32, i32, i32)> {
        std::mem::take(&mut self.data.display_event_trace)
            .into_iter()
            .filter_map(|event| match event {
                DisplayEvent::TransientVector { from, to } => Some((from.x, from.y, to.x, to.y)),
                DisplayEvent::StoredVector { from, to, .. } => Some((from.x, from.y, to.x, to.y)),
                _ => None,
            })
            .collect()
    }

    /// Import ordered file/error outcomes and retain their original notice order.
    pub fn import_ordered_files(&mut self, selection: FlashImportSelection) -> Result<(), String> {
        self.apply_ordered_import_files(selection)
    }

    /// Internal elapsed-time advancement with explicit input modifiers.
    fn advance_with_modifiers(&mut self, now: Instant, modifiers: KeyModifiers) {
        let program_load_was_active = self.data.input_sequence.program_load_active();
        let was_waiting_for_program_prompt = self.data.input_sequence.waiting_for_program_prompt();
        let elapsed = now.saturating_duration_since(self.data.last_update);
        self.data.last_update = now;
        if self.data.modal || self.data.runtime_error.is_some() {
            self.data
                .runtime
                .advance_erase_callbacks(&mut self.data.machine, elapsed);
        } else if let Err(error) = self.data.input_sequence.advance(
            elapsed,
            &mut self.data.keyboard,
            &mut self.data.machine,
            &mut self.data.runtime,
            modifiers,
        ) {
            self.cancel_sequenced_input();
            self.data.running = false;
            self.data.basic_prompt_ready = false;
            self.data.cursor_readiness.reset();
            self.data.input_notice = None;
            self.data.runtime_error = Some(error.to_string());
        }
        let display_events = self.data.runtime.take_display_events();
        #[cfg(test)]
        self.data
            .display_event_trace
            .extend(display_events.events.iter().copied());
        if self.data.running && self.data.runtime_error.is_none() {
            let program_load_active = self.data.input_sequence.program_load_active();
            let waiting_for_program_prompt = self.data.input_sequence.waiting_for_program_prompt();
            if program_load_active && !waiting_for_program_prompt {
                // A blinking input cursor while a line is typed is not a returned prompt.
                self.data.basic_prompt_ready = false;
            } else if (waiting_for_program_prompt && !was_waiting_for_program_prompt)
                || (program_load_was_active && !program_load_active)
            {
                self.data.cursor_readiness.reset();
                self.data.basic_prompt_ready = false;
            } else {
                self.data.basic_prompt_ready = self.data.cursor_readiness.observe(display_events);
                if self.data.basic_prompt_ready {
                    self.data.input_notice = None;
                    if self.data.input_sequence.resume_program_load_if_ready(
                        true,
                        &mut self.data.keyboard,
                        &mut self.data.machine,
                    ) {
                        self.data.basic_prompt_ready = false;
                        self.data.cursor_readiness.reset();
                    }
                }
            }
        } else {
            self.data.cursor_readiness.reset();
        }
        self.collect_screen_copies();
    }

    fn reset_machine(&mut self) {
        self.cancel_sequenced_input();
        self.data.basic_prompt_ready = false;
        self.data.cursor_readiness.reset();
        self.data.input_notice = None;
        let was_running = self.data.running;
        match self.data.machine.reset() {
            Ok(()) => {
                self.data.machine.erase_display();
                self.data.runtime.clear_display_events();
                self.data.runtime.restart(&mut self.data.machine);
                if !was_running {
                    self.data.runtime.pause();
                }
                self.data.running = was_running;
                self.data.runtime_error = None;
            }
            Err(error) => {
                self.data.runtime.pause();
                self.data.running = false;
                self.data.runtime_error = Some(format!("machine reset failed: {error}"));
            }
        }
    }

    fn cancel_auto_load_input(&mut self) {
        if self.data.input_sequence.auto_load_state() == AutoLoadState::Typing {
            self.cancel_sequenced_input();
        }
    }

    fn cancel_sequenced_input(&mut self) {
        self.data
            .input_sequence
            .cancel(&mut self.data.keyboard, &mut self.data.machine);
        self.data.last_modifiers = KeyModifiers::NONE;
    }

    fn launch_flash_program(&mut self, number: u8) {
        self.data.program_launch_file = None;
        self.data.program_launch_notice = None;
        self.data.auto_load_error = None;
        let is_binary_program = self
            .data
            .machine
            .flash_drive_directory()
            .iter()
            .find(|file| file.number == number && file.usage == Some("PROG"))
            .and_then(|file| match file.file_type {
                Some("ASCII") => Some(false),
                Some("BINARY") => Some(true),
                _ => None,
            });
        let rejection = if self.data.runtime_error.is_some() {
            Some("Reset the emulator before launching a program".to_owned())
        } else if !self.data.running {
            Some("Start the emulator and wait for the BASIC prompt before launching".to_owned())
        } else if !self.data.basic_prompt_ready {
            Some("Wait for the BASIC cursor or INPUT prompt before launching".to_owned())
        } else if is_binary_program.is_none() {
            Some(format!(
                "Flash Drive file {number} is not an ASCII or BINARY PROG"
            ))
        } else {
            None
        };
        if let Some(message) = rejection {
            self.data.program_launch_notice = Some((true, message));
        } else {
            let queue_result = if is_binary_program == Some(true) {
                self.data.input_sequence.queue_binary_program_load(
                    number,
                    &mut self.data.keyboard,
                    &mut self.data.machine,
                )
            } else {
                self.data.input_sequence.queue_program_load(
                    number,
                    &mut self.data.keyboard,
                    &mut self.data.machine,
                )
            };
            if let Err(error) = queue_result {
                self.data.program_launch_notice =
                    Some((true, format!("Program launch failed: {error}")));
            } else {
                self.data.program_launch_file = Some(number);
                self.data.basic_prompt_ready = false;
                self.data.cursor_readiness.reset();
            }
        }
    }

    /// Immediate state changes return None; callers must execute and complete a
    /// returned file effect before processing another command or advancing time.
    /// Advance to the current host time before a Run/Pause transition: this
    /// method does not account for the preceding interval under the old state.
    pub fn apply(&mut self, command: Command) -> Result<Option<FileEffect>, String> {
        match command {
            Command::Run(run) => {
                if run != self.data.running {
                    self.data.basic_prompt_ready = false;
                    self.data.cursor_readiness.reset();
                    self.data.input_notice = None;
                    self.data.runtime.clear_display_events();
                    if run {
                        self.data
                            .runtime
                            .resume()
                            .map_err(|error| error.to_string())?;
                    } else {
                        self.cancel_auto_load_input();
                        self.data.runtime.pause();
                    }
                    self.data.running = run;
                }
            }
            Command::Reset => self.reset_machine(),
            Command::Speed(index) => {
                let result = SPEED_FACTORS
                    .get(index)
                    .ok_or_else(|| format!("invalid speed index: {index}"))
                    .and_then(|factor| {
                        self.data
                            .runtime
                            .set_speed_factor(f64::from(*factor))
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => {
                        self.data.speed_index = index;
                        self.data.settings_error = None;
                    }
                    Err(error) => self.data.settings_error = Some(error),
                }
            }
            Command::Granularity(value) => self
                .data
                .runtime
                .set_display_granularity(&mut self.data.machine, value),
            Command::CapsLock(value) => {
                if value != self.data.caps_lock {
                    self.data.machine.toggle_caps_lock();
                    self.data.caps_lock = value;
                }
            }
            Command::PromptReady { ready, request } => {
                if ready && !self.data.cursor_readiness.is_ready() {
                    return Err(
                        "BASIC readiness is detected from a transient cursor or INPUT-prompt mark"
                            .to_owned(),
                    );
                }
                self.data.basic_prompt_ready = ready;
                self.data.prompt_request = request;
                if !ready {
                    self.data.cursor_readiness.reset();
                    self.data.runtime.clear_display_events();
                    self.data.input_notice = None;
                }
            }
            Command::AutoLoad => {
                self.data.program_launch_file = None;
                self.data.program_launch_notice = None;
                let error = if !self.data.running || self.data.runtime_error.is_some() {
                    Some("Start the emulator before using Auto Load".to_owned())
                } else if !self.data.basic_prompt_ready {
                    Some(
                        "Wait for the BASIC cursor or INPUT prompt before using Auto Load"
                            .to_owned(),
                    )
                } else {
                    self.data
                        .input_sequence
                        .queue_auto_load(&mut self.data.keyboard, &mut self.data.machine)
                        .err()
                        .map(str::to_owned)
                };
                if error.is_none() {
                    self.data.basic_prompt_ready = false;
                    self.data.cursor_readiness.reset();
                }
                self.data.auto_load_error = error;
            }
            Command::Launch(number) => self.launch_flash_program(number),
            Command::Input(frame) => {
                if !frame.focused {
                    self.cancel_auto_load_input();
                    if self.data.modal {
                        self.release_input();
                    }
                }
                if !self.data.modal {
                    if self.data.running {
                        let submitted = frame_has_return(&frame);
                        frame.apply(
                            &mut self.data.input_sequence,
                            &mut self.data.keyboard,
                            &mut self.data.machine,
                        );
                        if submitted {
                            self.data.basic_prompt_ready = false;
                            self.data.cursor_readiness.reset();
                        }
                        self.data.last_modifiers = frame.modifiers;
                        if frame_has_press(&frame) {
                            self.data.input_notice = None;
                        }
                    } else {
                        if frame_has_press(&frame) {
                            self.data.input_notice =
                                Some("Start the emulator before typing".into());
                        }
                        if frame.focused {
                            self.release_host_input(&frame);
                        } else {
                            self.release_input();
                        }
                    }
                }
            }
            Command::Modal(value) => {
                self.data.modal = value;
            }
            Command::BeginFileOperation(id) => {
                self.data.modal = true;
                self.data.file_operation = id;
            }
            Command::ImportFiles(files) => self.apply_import_files(files, Vec::new()),
            Command::ImportSelectedFiles(selection) => {
                self.apply_ordered_import_files(selection)?
            }
            Command::ImportArchive(path) => return Ok(Some(FileEffect::ImportArchive(path))),
            Command::ExportFile { number, path } => {
                match self.data.machine.export_flash_file(number) {
                    Ok(file) => return Ok(Some(FileEffect::ExportFile { number, path, file })),
                    Err(error) => {
                        self.data.export_notice =
                            Some((true, format!("Flash Drive export failed: {error}")));
                    }
                }
            }
            Command::ExportArchive(path) => return Ok(Some(FileEffect::ExportArchive(path))),
            Command::RequestScreenCopy => {
                let pixels = self
                    .data
                    .runtime
                    .display(&mut self.data.machine)
                    .pixels()
                    .to_vec();
                self.capture_screen(Arc::from(pixels));
            }
            Command::SaveScreen { id, path } => {
                let index = self
                    .data
                    .screen_copy_requests
                    .iter()
                    .position(|(number, _)| *number == id)
                    .ok_or_else(|| format!("unknown screen copy request: {id}"))?;
                let (_, pixels) = self.data.screen_copy_requests.remove(index);
                return Ok(Some(FileEffect::SaveScreen { path, pixels }));
            }
            Command::CancelScreenCopy(id) => {
                let index = self
                    .data
                    .screen_copy_requests
                    .iter()
                    .position(|(number, _)| *number == id)
                    .ok_or_else(|| format!("unknown screen copy request: {id}"))?;
                self.data.screen_copy_requests.remove(index);
                self.screen_notice((false, "Screen copy cancelled".to_owned()));
            }
        }
        Ok(None)
    }

    /// Record the host's completed effect notice using the existing overflow policy.
    pub fn complete_file_effect(&mut self, outcome: FileOutcome) {
        match outcome {
            FileOutcome::Import(notice) => self.data.import_notices = vec![notice],
            FileOutcome::Export(notice) => self.data.export_notice = Some(notice),
            FileOutcome::Screen(notice) => self.screen_notice(notice),
        }
    }

    fn apply_import_files(&mut self, files: Vec<FlashDriveFile>, errors: Vec<String>) {
        let results = self.data.machine.import_flash_files(&files);
        let count = results.iter().filter(|result| result.is_ok()).count();
        self.data.import_notices =
            vec![(count == 0, format!("{count} Flash Drive file(s) imported"))];
        for (file, result) in files.iter().zip(results) {
            if let Err(error) = result {
                self.data
                    .import_notices
                    .push((true, format!("Import failed for {}: {error}", file.name)));
            }
        }
        self.data
            .import_notices
            .extend(errors.into_iter().map(|error| (true, error)));
    }

    fn apply_ordered_import_files(
        &mut self,
        selection: FlashImportSelection,
    ) -> Result<(), String> {
        selection.validate_order()?;

        let results = self.data.machine.import_flash_files(&selection.files);
        if results.len() != selection.files.len() {
            return Err("Flash Drive import returned an unexpected result count".to_owned());
        }
        let count = results.iter().filter(|result| result.is_ok()).count();
        self.data.import_notices =
            vec![(count == 0, format!("{count} Flash Drive file(s) imported"))];
        for item in selection.order {
            match item {
                FlashImportOrder::File(index) => {
                    if let Err(error) = &results[index] {
                        self.data.import_notices.push((
                            true,
                            format!("Import failed for {}: {error}", selection.files[index].name),
                        ));
                    }
                }
                FlashImportOrder::Error(index) => {
                    self.data
                        .import_notices
                        .push((true, selection.errors[index].clone()));
                }
            }
        }
        Ok(())
    }

    fn release_input(&mut self) {
        InputFrame::unfocused().apply(
            &mut self.data.input_sequence,
            &mut self.data.keyboard,
            &mut self.data.machine,
        );
        self.data.last_modifiers = KeyModifiers::NONE;
    }

    fn release_host_input(&mut self, frame: &InputFrame) {
        if frame.reset_input {
            self.release_input();
            return;
        }

        let actions = frame
            .actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    InputAction::ManualRelease(_)
                        | InputAction::SequencedManualRelease(_)
                        | InputAction::PanelRelease(_)
                )
            })
            .cloned()
            .collect();
        InputFrame {
            actions,
            modifiers: KeyModifiers::NONE,
            focused: true,
            reset_input: false,
        }
        .apply(
            &mut self.data.input_sequence,
            &mut self.data.keyboard,
            &mut self.data.machine,
        );
    }

    fn screen_notice(&mut self, notice: (bool, String)) {
        if self.data.screen_copy_notices.len() == MAX_NOTICES {
            self.data.screen_copy_notices.remove(0);
        }
        self.data.screen_copy_notices.push(notice);
    }

    fn capture_screen(&mut self, pixels: Arc<[u8]>) {
        if self.data.screen_copy_requests.len() == MAX_SCREEN_COPIES {
            self.data.dropped_screen_copies = self.data.dropped_screen_copies.saturating_add(1);
            self.restore_overflow_notice();
            return;
        }
        if self.data.screen_copy_requests.is_empty() {
            self.data.dropped_screen_copies = 0;
        }
        let id = self.data.next_screen_copy;
        self.data.next_screen_copy = self.data.next_screen_copy.wrapping_add(1);
        self.data.screen_copy_requests.push((id, pixels));
    }

    fn collect_screen_copies(&mut self) {
        #[cfg(feature = "webgpu")]
        for warning in self.data.runtime.take_screen_copy_warnings() {
            self.screen_notice((false, warning));
        }
        let (copies, dropped) = self.data.runtime.take_screen_copies(&mut self.data.machine);
        for pixels in copies {
            self.capture_screen(Arc::from(pixels));
        }
        if dropped != 0 {
            self.data.dropped_screen_copies =
                self.data.dropped_screen_copies.saturating_add(dropped);
            self.restore_overflow_notice();
        }
    }

    fn restore_overflow_notice(&mut self) {
        if self.data.dropped_screen_copies == 0 {
            return;
        }
        let message = format!(
            "At least {} screen copy request(s) could not be saved: queue full",
            self.data.dropped_screen_copies
        );
        if let Some((_, previous)) =
            self.data
                .screen_copy_notices
                .iter_mut()
                .find(|(_, message)| {
                    message.contains("screen copy request(s) could not be saved: queue full")
                })
        {
            *previous = message;
        } else {
            if self.data.screen_copy_notices.len() == MAX_NOTICES {
                self.data.screen_copy_notices.pop();
            }
            self.data.screen_copy_notices.insert(0, (true, message));
        }
    }

    /// Publish owned metadata and cached pixels, collecting pending machine captures.
    pub fn snapshot(&mut self) -> Arc<Snapshot> {
        self.collect_screen_copies();
        self.restore_overflow_notice();
        let granularity = self
            .data
            .runtime
            .display(&mut self.data.machine)
            .granularity();
        #[cfg(feature = "webgpu")]
        let revision = self.data.runtime.presentation_revision();
        #[cfg(not(feature = "webgpu"))]
        let revision = self.data.runtime.display(&mut self.data.machine).revision();
        let pixels = match &self.data.cached_pixels {
            Some((cached_revision, pixels)) if *cached_revision == revision => Arc::clone(pixels),
            _ => {
                let pixels = Arc::from(self.data.runtime.display(&mut self.data.machine).pixels());
                self.data.cached_pixels = Some((revision, Arc::clone(&pixels)));
                pixels
            }
        };
        let directory = self.data.machine.flash_drive_directory();
        let export_names = directory
            .iter()
            .map(|file| {
                (
                    file.number,
                    self.data
                        .machine
                        .export_flash_file_name(file.number)
                        .map_err(|error| error.to_string()),
                )
            })
            .collect();
        Arc::new(Snapshot {
            pixels,
            revision,
            indicators: self.data.machine.indicators(),
            registers: self.data.machine.registers(),
            directory,
            export_names,
            granularity,
            running: self.data.running,
            caps_lock: self.data.caps_lock,
            speed_index: self.data.speed_index,
            settings_error: self.data.settings_error.clone(),
            runtime_error: self.data.runtime_error.clone(),
            render_error: {
                #[cfg(feature = "webgpu")]
                {
                    self.data.runtime.render_error().map(str::to_owned)
                }
                #[cfg(not(feature = "webgpu"))]
                {
                    None
                }
            },
            webgpu_active: {
                #[cfg(feature = "webgpu")]
                {
                    self.data.runtime.webgpu_active()
                }
                #[cfg(not(feature = "webgpu"))]
                {
                    false
                }
            },
            extended_webgpu_active: {
                #[cfg(feature = "webgpu")]
                {
                    self.data.runtime.extended_webgpu_active()
                }
                #[cfg(not(feature = "webgpu"))]
                {
                    false
                }
            },
            basic_prompt_ready: self.data.basic_prompt_ready,
            prompt_request: self.data.prompt_request,
            input_notice: self.data.input_notice.clone(),
            program_launch_file: self.data.program_launch_file,
            program_launch_notice: self.data.program_launch_notice.clone(),
            auto_load_error: self.data.auto_load_error.clone(),
            auto_load_state: self.data.input_sequence.auto_load_state(),
            import_notices: self.data.import_notices.clone(),
            export_notice: self.data.export_notice.clone(),
            screen_copy_notices: self.data.screen_copy_notices.clone(),
            screen_copy_requests: self.data.screen_copy_requests.clone(),
            coalesced_time: self.data.coalesced_time,
            busy: self.data.modal,
            file_operation: self.data.file_operation,
        })
    }

    /// Returns whether this session currently owns a pressed manual or panel key.
    pub fn has_held_key(&self) -> bool {
        !self.data.keyboard.is_idle()
    }

    #[cfg(all(test, feature = "wasm"))]
    pub(crate) fn configure_keyboard_pia_for_test(
        &mut self,
    ) -> Result<(), crate::machine::MachineBusError> {
        self.data.machine.write_byte(0x87a9, 0x04)?;
        self.data.machine.write_byte(0x87ab, 0x04)?;
        self.data.machine.keyboard_release();
        Ok(())
    }

    #[cfg(all(test, feature = "wasm"))]
    pub(crate) fn keyboard_pia_for_test(
        &mut self,
    ) -> Result<(u8, u8), crate::machine::MachineBusError> {
        Ok((
            self.data.machine.read_byte(0x87a8)?,
            self.data.machine.read_byte(0x87aa)?,
        ))
    }

    /// Coalesce CPU catch-up to one tick while accounting for all erase wall time.
    pub fn advance_realtime(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.data.last_update);
        let logical = elapsed.min(TICK);
        let omitted = elapsed.saturating_sub(logical);
        self.advance_with_modifiers(self.data.last_update + logical, self.data.last_modifiers);
        if !omitted.is_zero() {
            self.data
                .runtime
                .advance_erase_callbacks(&mut self.data.machine, omitted);
            if self.data.running && !self.data.modal {
                self.data.coalesced_time = self.data.coalesced_time.saturating_add(omitted);
            }
        }
        self.data.last_update = now;
    }
}
