//! Native presentation and nonblocking communication with the emulation owner.
//!
//! Widgets read immutable snapshots and submit ordered commands. Folder dialogs
//! and host-file preparation run as a single background job, never in a repaint.
//! The deterministic test backend exercises the same widgets without wall clocks.

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

#[path = "flash_directory.rs"]
mod flash_directory;

use super::host_dialogs::{
    confirm_flash_storage_archive_replacement, pick_flash_storage_archive, save_flash_archive,
    save_flash_file, save_screen_copy,
};
use super::native_backend::{Backend, WorkerBackend, WorkloadSample};
use super::*;
#[cfg(feature = "webgpu")]
use tek4051_emulator::emulation::webgpu::{DeferredWebGpuSubmissions, WebGpuVectorRenderer};
use tek4051_emulator::{
    emulation::{Command, SPEED_FACTORS, Snapshot},
    emulation_input::{InputFrame, KeyModifiers},
};

const WORKLOAD_SMOOTHING_LIMIT: Duration = Duration::from_millis(50);

fn workload_sample_segments(samples: &[WorkloadSample]) -> Vec<&[WorkloadSample]> {
    let mut segments = Vec::new();
    let mut start = 0;
    for index in 1..samples.len() {
        let gap = samples[index].elapsed_seconds - samples[index - 1].elapsed_seconds;
        // Do not draw a trend across time without measurements.
        if gap > WORKLOAD_SMOOTHING_LIMIT.as_secs_f64() + 1e-6 {
            segments.push(&samples[start..index]);
            start = index;
        }
    }
    if start < samples.len() {
        segments.push(&samples[start..]);
    }
    segments
}

pub(super) struct Tek4051App<B: Backend = WorkerBackend> {
    pub(super) backend: B,
    snapshot: Arc<Snapshot>,
    workload_samples: Arc<[WorkloadSample]>,
    workload_timestamp: Option<f64>,
    workload_received_at: Instant,
    frame_time: Instant,
    pub(super) display_texture: egui::TextureHandle,
    pub(super) webgpu_texture_id: Option<egui::TextureId>,
    webgpu_notice: Option<String>,
    #[cfg(feature = "webgpu")]
    webgpu_submissions: Option<Arc<DeferredWebGpuSubmissions>>,
    pub(super) display_revision: u64,
    keyboard_ui: KeyboardInput,
    panel_view: FrontPanel,
    pub(super) export_number: u8,
    pub(super) show_flash_directory: bool,
    pub(super) basic_prompt_ready: bool,
    last_input_state: Option<(bool, KeyModifiers, bool)>,
    prompt_request_pending: Option<u64>,
    next_prompt_request: u64,
    pub(super) directory_list_focused: bool,
    pub(super) selected_directory_file: Option<u8>,
    pub(super) native_control_focused: bool,
    worker_error: Option<String>,
    file_job: Option<mpsc::Receiver<Result<FileSelection, String>>>,
    pending_file: Option<(u64, FileRequest)>,
    next_file_operation: u64,
    file_notice: Option<(bool, String)>,
    screen_in_flight: Option<u64>,
}

#[derive(Default)]
pub(super) struct Requests {
    pub(super) import: bool,
    pub(super) import_archive: bool,
    pub(super) export: bool,
    pub(super) export_archive: bool,
}

enum FileSelection {
    Import(Option<FlashImportSelection>),
    Archive(Option<PathBuf>),
    Export(u8, Option<PathBuf>),
    ExportArchive(Option<PathBuf>),
    Screen(u64, Option<PathBuf>),
}

enum FileRequest {
    Import,
    Archive,
    Export(u8),
    ExportArchive,
    Screen(u64),
}

#[cfg(feature = "webgpu")]
fn initialize_webgpu_display<B: Backend>(
    context: &eframe::CreationContext<'_>,
    backend: &mut B,
) -> Result<Option<(egui::TextureId, Arc<DeferredWebGpuSubmissions>)>, String> {
    let render_state = context
        .wgpu_render_state
        .as_ref()
        .ok_or_else(|| "eframe did not provide WebGPU render state".to_owned())?;
    let device = Arc::new(render_state.device.clone());
    let queue = Arc::new(render_state.queue.clone());
    let submissions = Arc::new(DeferredWebGpuSubmissions::new(Arc::clone(&queue)));
    let mut renderer = WebGpuVectorRenderer::new(
        Arc::clone(&device),
        Arc::clone(&queue),
        CpuReferenceRenderer::WIDTH as u32,
        CpuReferenceRenderer::HEIGHT as u32,
    )
    .map_err(|error| error.to_string())?;
    renderer.set_deferred_submissions(Arc::clone(&submissions));
    let texture_id = render_state.renderer.write().register_native_texture(
        &device,
        renderer.target_view(),
        eframe::wgpu::FilterMode::Linear,
    );
    backend.enable_webgpu_renderer(renderer)?;
    Ok(Some((texture_id, submissions)))
}

impl<B: Backend> Tek4051App<B> {
    pub(super) fn with_backend(
        context: &eframe::CreationContext<'_>,
        mut backend: B,
    ) -> Result<Self, String> {
        let presentation = backend
            .snapshot()?
            .ok_or_else(|| "emulation worker did not publish its initial state".to_owned())?;
        let display_texture = context.egui_ctx.load_texture(
            "tek4051-display",
            egui::ImageData::Color(presentation.image),
            egui::TextureOptions::LINEAR,
        );
        let workload_samples = Arc::clone(&presentation.workload_samples);
        let workload_timestamp = workload_samples.last().map(|sample| sample.elapsed_seconds);
        let workload_received_at = Instant::now();
        #[cfg(feature = "webgpu")]
        let (webgpu_texture_id, webgpu_notice, webgpu_submissions) =
            match initialize_webgpu_display(context, &mut backend) {
                Ok(Some((texture_id, submissions))) => (Some(texture_id), None, Some(submissions)),
                Ok(None) => (None, None, None),
                Err(error) => (None, Some(error), None),
            };
        #[cfg(not(feature = "webgpu"))]
        let (webgpu_texture_id, webgpu_notice) = (None, None);
        let snapshot = presentation.snapshot;
        Ok(Self {
            display_revision: snapshot.revision,
            snapshot,
            workload_samples,
            workload_timestamp,
            workload_received_at,
            frame_time: workload_received_at,
            backend,
            display_texture,
            webgpu_texture_id,
            webgpu_notice,
            #[cfg(feature = "webgpu")]
            webgpu_submissions,
            keyboard_ui: KeyboardInput::default(),
            panel_view: FrontPanel::default(),
            export_number: 1,
            show_flash_directory: false,
            basic_prompt_ready: false,
            last_input_state: None,
            prompt_request_pending: None,
            next_prompt_request: 0,
            directory_list_focused: false,
            selected_directory_file: None,
            native_control_focused: false,
            worker_error: None,
            file_job: None,
            pending_file: None,
            next_file_operation: 0,
            file_notice: None,
            screen_in_flight: None,
        })
    }

    #[cfg(feature = "webgpu")]
    pub(super) fn set_webgpu_notice(&mut self, notice: String) {
        self.webgpu_notice = Some(notice);
    }

    fn send(&mut self, command: Command) {
        if self.worker_error.is_some() && matches!(&command, Command::Input(_)) {
            return;
        }
        if let Err(error) = self.backend.command(command) {
            self.basic_prompt_ready = false;
            self.worker_error = Some(error);
        }
    }

    fn input_frame_changed(&mut self, frame: &InputFrame) -> bool {
        let state = (frame.focused, frame.modifiers, frame.reset_input);
        // State-only frames matter on transitions; unchanged frames only occupy worker capacity.
        let changed = !frame.actions.is_empty() || self.last_input_state != Some(state);
        self.last_input_state = Some(state);
        changed
    }

    fn send_input_frame(&mut self, frame: InputFrame) {
        if self.input_frame_changed(&frame) {
            self.send(Command::Input(Box::new(frame)));
        }
    }

    pub(super) fn refresh(&mut self) {
        match self.backend.snapshot() {
            Ok(Some(presentation)) => {
                let snapshot = presentation.snapshot;
                let workload_timestamp = presentation
                    .workload_samples
                    .last()
                    .map(|sample| sample.elapsed_seconds);
                if workload_timestamp != self.workload_timestamp {
                    self.workload_timestamp = workload_timestamp;
                    self.workload_received_at = Instant::now();
                }
                self.workload_samples = Arc::clone(&presentation.workload_samples);
                if !snapshot.webgpu_active && snapshot.render_error.is_some() {
                    self.webgpu_texture_id = None;
                }
                if self.screen_in_flight.is_some_and(|id| {
                    !snapshot
                        .screen_copy_requests
                        .iter()
                        .any(|(pending, _)| *pending == id)
                }) {
                    self.screen_in_flight = None;
                }
                if !snapshot.webgpu_active && snapshot.revision != self.display_revision {
                    self.display_texture.set(
                        egui::ImageData::Color(presentation.image),
                        egui::TextureOptions::LINEAR,
                    );
                    self.display_revision = snapshot.revision;
                }
                if self.prompt_request_pending == Some(snapshot.prompt_request) {
                    self.prompt_request_pending = None;
                }
                if !snapshot.running || snapshot.runtime_error.is_some() {
                    self.basic_prompt_ready = false;
                } else if self.prompt_request_pending.is_none() {
                    self.basic_prompt_ready = snapshot.basic_prompt_ready;
                }
                self.snapshot = snapshot;
            }
            Ok(None) => {}
            Err(error) => {
                self.basic_prompt_ready = false;
                self.worker_error = Some(error);
            }
        }
    }

    pub(super) fn begin_frame(&mut self, context: &egui::Context, now: Instant) {
        self.frame_time = now;
        if !context.input(|input| input.focused) {
            self.send_input_frame(InputFrame::unfocused());
        }
        self.backend.advance(now);
        self.refresh();
    }

    pub(super) fn render(&mut self, context: &egui::Context) -> Requests {
        let mut requests = Requests::default();
        let mut commands = Vec::new();
        let mut native_tab_navigation = self.show_flash_directory || self.native_control_focused;
        let mut export_number_editing = false;
        let mut directory_keyboard_focus = false;
        let mut launch = None;
        let mut auto_load = false;
        let file_busy =
            self.pending_file.is_some() || self.file_job.is_some() || self.snapshot.busy;
        let connected = self.backend.is_connected();
        self.keyboard_ui.consume_ui_activation_repeats(context);
        self.keyboard_ui
            .consume_directory_navigation(context, self.directory_list_focused);
        egui::TopBottomPanel::top("display_controls").show(context, |ui| {
            ui.horizontal(|ui| {
                let run_label = if self.snapshot.running { "Pause" } else { "Start" };
                if ui
                    .add_enabled(
                        connected && self.snapshot.runtime_error.is_none(),
                        egui::Button::new(run_label),
                    )
                    .clicked()
                {
                    commands.push(Command::Run(!self.snapshot.running));
                    commands.push(self.prompt_command(false));
                }
                ui.label("Display granularity:");
                let mut granularity = self.snapshot.granularity;
                ui.selectable_value(&mut granularity, DisplayGranularity::Fine, "Fine");
                ui.selectable_value(&mut granularity, DisplayGranularity::Coarse, "Coarse");
                ui.selectable_value(&mut granularity, DisplayGranularity::Mixed, "Mixed");
                if granularity != self.snapshot.granularity {
                    commands.push(Command::Granularity(granularity));
                }
                ui.label("CPU speed:");
                let mut speed_index = self.snapshot.speed_index;
                egui::ComboBox::from_id_salt("cpu_speed")
                    .selected_text(format!("x{}", SPEED_FACTORS[speed_index]))
                    .show_ui(ui, |ui| {
                        for (index, factor) in SPEED_FACTORS.iter().enumerate() {
                            ui.selectable_value(
                                &mut speed_index,
                                index,
                                format!("x{factor}"),
                            );
                        }
                    });
                if speed_index != self.snapshot.speed_index {
                    commands.push(Command::Speed(speed_index));
                }
                let mut caps_lock = self.snapshot.caps_lock;
                if ui.checkbox(&mut caps_lock, "Caps Lock").changed() {
                    commands.push(Command::CapsLock(caps_lock));
                }
                if ui.add_enabled(connected, egui::Button::new("Reset")).clicked() {
                    commands.push(Command::Reset);
                    commands.push(self.prompt_command(false));
                }
                if let Some(error) = &self.snapshot.runtime_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("Emulation stopped: {error}"));
                } else {
                    ui.label(format!("Tape: GPIB address 5 | PC {:04X}", self.snapshot.registers.pc));
                }
            });
            if let Some(error) = &self.worker_error {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Emulation error: {error}"));
            }
            if let Some(error) = &self.snapshot.settings_error {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Speed setting: {error}"));
            }
            if let Some(error) = self
                .snapshot
                .render_error
                .as_deref()
                .or(self.webgpu_notice.as_deref())
            {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Display renderer: {error}"));
            }
            if !self.snapshot.coalesced_time.is_zero() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "Timing: {:.3}s of catch-up skipped; reduce CPU speed if this keeps increasing.",
                        self.snapshot.coalesced_time.as_secs_f64()
                    ),
                );
            }
            let typing = self.snapshot.auto_load_state == AutoLoadState::Typing;
            ui.horizontal(|ui| {
                requests.import = ui
                    .add_enabled(
                        connected && !typing && !file_busy,
                        egui::Button::new("Import folder contents..."),
                    )
                    .clicked();
                auto_load = ui
                    .add_enabled(
                        connected && self.snapshot.running && self.snapshot.runtime_error.is_none()
                            && !typing && !file_busy && context.input(|input| input.focused),
                        egui::Button::new("Auto Load"),
                    )
                    .clicked();
                ui.label(
                    "Auto Load uses file 1; directory launch uses OLD@5: or CALL \"BOLD\",5.",
                );
            });
            ui.label("Imports files and ZIPs directly in the selected folder; subfolders are not scanned.");
            ui.horizontal(|ui| {
                requests.import_archive = ui
                    .add_enabled(
                        connected && !typing && !file_busy,
                        egui::Button::new("Import Flash Drive archive..."),
                    )
                    .clicked();
                ui.label("Replaces all stored files and aborts addressed tape transfers after confirmation.");
            });
            ui.horizontal(|ui| {
                ui.label("Export file number:");
                let number = ui.add_enabled(
                    connected && !typing && !file_busy,
                    egui::DragValue::new(&mut self.export_number).range(1..=254),
                );
                export_number_editing = number.has_focus() || number.lost_focus();
                requests.export = ui
                    .add_enabled(
                        connected && !typing && !file_busy,
                        egui::Button::new("Export Flash Drive file..."),
                    )
                    .clicked();
                requests.export_archive = ui
                    .add_enabled(
                        connected && !typing && !file_busy,
                        egui::Button::new("Export Flash Drive archive..."),
                    )
                    .clicked();
                let directory = ui.checkbox(&mut self.show_flash_directory, "Show directory");
                directory_keyboard_focus = directory.has_focus() || directory.lost_focus();
            });
            if file_busy {
                ui.label("File operation in progress...");
            }
            for (error, notice) in self.file_notice.iter()
                .chain(self.snapshot.export_notice.iter())
                .chain(self.snapshot.import_notices.iter())
                .chain(self.snapshot.screen_copy_notices.iter())
            {
                notice_label(ui, *error, notice);
            }
            if let Some((error, notice)) = &self.snapshot.program_launch_notice {
                notice_label(ui, *error, notice);
            } else {
                match (self.snapshot.auto_load_state, self.snapshot.program_launch_file) {
                    (AutoLoadState::Idle, _) => {}
                    (AutoLoadState::Typing, Some(number)) => { ui.label(format!("Launching Flash Drive program {number}")); }
                    (AutoLoadState::Sent, Some(number)) => { ui.label(format!("Launch commands sent for file {number}; execution may still fail")); }
                    (AutoLoadState::Cancelled, Some(number)) => { notice_label(ui, true, &format!("Launch of file {number} cancelled; a partial command may remain in BASIC")); }
                    (AutoLoadState::Typing, None) => { ui.label("Auto Load: typing file-1 commands"); }
                    (AutoLoadState::Sent, None) => { ui.label("Auto Load commands sent (file 1)"); }
                    (AutoLoadState::Cancelled, None) => { notice_label(ui, true, "Auto Load cancelled; a partial command may remain in the BASIC editor"); }
                }
            }
            if let Some(error) = &self.snapshot.auto_load_error {
                notice_label(ui, true, &format!("Auto Load: {error}"));
            }
            ui.label(if !self.snapshot.running {
                "Emulator stopped; keyboard input is not delivered."
            } else if self.basic_prompt_ready {
                "BASIC prompt detected."
            } else {
                "BASIC prompt not detected; keyboard input still reaches the machine."
            });
            if let Some(notice) = &self.snapshot.input_notice {
                notice_label(ui, true, notice);
            }
            let indicators = self.snapshot.indicators;
            ui.horizontal(|ui| {
                for (label, active) in [
                    ("BUSY", indicators.busy),
                    ("BREAK", indicators.break_active),
                    ("I/O", indicators.io),
                    ("POWER", self.snapshot.running),
                ] {
                    ui.colored_label(
                        if active { egui::Color32::from_rgb(0, 238, 0) }
                        else { egui::Color32::from_rgb(0, 85, 0) },
                        label,
                    );
                }
            });
            if self.panel_view.show(ui) {
                commands.push(Command::RequestScreenCopy);
            }
        });
        if self.show_flash_directory {
            let action = egui::SidePanel::right("flash_drive_directory")
                .default_width(360.0)
                .show(context, |ui| {
                    flash_directory::show(
                        ui,
                        &self.snapshot.directory,
                        self.snapshot.running,
                        self.basic_prompt_ready,
                        self.snapshot.input_notice.as_deref(),
                        &mut self.directory_list_focused,
                        &mut self.selected_directory_file,
                    )
                })
                .inner;
            directory_keyboard_focus |= action.keyboard_activation_owned;
            launch = action.launch_file_number;
        } else {
            self.directory_list_focused = false;
        }
        egui::TopBottomPanel::bottom("emulation_workload")
            .default_height(142.0)
            .min_height(110.0)
            .resizable(true)
            .show(context, |ui| {
                let peak = self
                    .workload_samples
                    .iter()
                    .map(|sample| sample.workload_percent)
                    .fold(0.0, f32::max);
                let average = if self.workload_samples.is_empty() {
                    0.0
                } else {
                    self.workload_samples
                        .iter()
                        .map(|sample| sample.workload_percent)
                        .sum::<f32>()
                        / self.workload_samples.len() as f32
                };
                ui.horizontal(|ui| {
                    ui.strong("Emulation workload");
                    ui.label(format!(
                        "x{} | average {average:.0}% | peak {peak:.0}% | 100% = one 10 ms budget",
                        SPEED_FACTORS[self.snapshot.speed_index]
                    ));
                });
                workload_graph(
                    ui,
                    &self.workload_samples,
                    self.workload_received_at,
                    self.frame_time,
                );
            });
        egui::CentralPanel::default().show(context, |ui| {
            let size = egui::vec2(
                CpuReferenceRenderer::WIDTH as f32,
                CpuReferenceRenderer::HEIGHT as f32,
            );
            let available = ui.available_size();
            let scale = (available.x / size.x).min(available.y / size.y).max(0.0);
            let image = match self.webgpu_texture_id {
                Some(texture_id) => egui::Image::new((texture_id, size * scale)),
                None => egui::Image::new(&self.display_texture).fit_to_exact_size(size * scale),
            };
            ui.add(image);
        });
        native_tab_navigation |= self.show_flash_directory;
        if !native_tab_navigation
            && context.input(|input| input.key_pressed(egui::Key::Tab))
            && let Some(focused) = context.memory(|memory| memory.focused())
        {
            context.memory_mut(|memory| memory.surrender_focus(focused));
        }

        let mut owned_keys = Vec::new();
        if native_tab_navigation {
            owned_keys.push(egui::Key::Tab);
        }
        if directory_keyboard_focus {
            owned_keys.extend([
                egui::Key::Space,
                egui::Key::Enter,
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
            ]);
        }
        let (events, modifiers, focused) =
            context.input(|input| (input.raw.events.clone(), input.modifiers, input.focused));
        let frame = self.keyboard_ui.translate_frame(
            &self.panel_view,
            (&events, modifiers, focused),
            export_number_editing,
            &owned_keys,
        );
        if self.input_frame_changed(&frame) {
            commands.push(Command::Input(Box::new(frame)));
        }
        if let Some(number) = launch {
            commands.push(Command::Launch(number));
        }
        if auto_load {
            commands.push(Command::AutoLoad);
        }
        for command in commands {
            self.send(command);
        }
        self.native_control_focused = context.memory(|memory| memory.focused().is_some());
        requests
    }

    pub(super) fn update_at(&mut self, context: &egui::Context, now: Instant) {
        self.begin_frame(context, now);
        self.poll_file_job();
        self.open_pending_file_job();
        let requests = self.render(context);
        if self.file_job.is_none()
            && self.pending_file.is_none()
            && !self.snapshot.busy
            && self.worker_error.is_none()
        {
            if requests.import {
                self.queue_file_job(FileRequest::Import);
            } else if requests.import_archive {
                self.queue_file_job(FileRequest::Archive);
            } else if requests.export {
                self.queue_file_job(FileRequest::Export(self.export_number));
            } else if requests.export_archive {
                self.queue_file_job(FileRequest::ExportArchive);
            } else if self.screen_in_flight.is_none()
                && let Some((id, _)) = self.snapshot.screen_copy_requests.first()
            {
                let id = *id;
                self.screen_in_flight = Some(id);
                self.queue_file_job(FileRequest::Screen(id));
            }
        }
        self.refresh();
        context.request_repaint_after(Duration::from_millis(10));
    }

    fn prompt_command(&mut self, ready: bool) -> Command {
        self.next_prompt_request += 1;
        self.prompt_request_pending = Some(self.next_prompt_request);
        self.basic_prompt_ready = ready;
        Command::PromptReady {
            ready,
            request: self.next_prompt_request,
        }
    }

    fn queue_file_job(&mut self, request: FileRequest) {
        self.next_file_operation += 1;
        let id = self.next_file_operation;
        self.pending_file = Some((id, request));
        self.file_notice = None;
        self.send(Command::BeginFileOperation(id));
    }

    fn open_pending_file_job(&mut self) {
        let Some((id, _)) = &self.pending_file else {
            return;
        };
        if !self.snapshot.busy || self.snapshot.file_operation != *id {
            return;
        }
        // The acknowledged snapshot was built after CPU execution was suspended.
        // Both filename selection and the later write therefore see the frozen store.
        let (_, request) = self
            .pending_file
            .take()
            .expect("pending file operation was checked");
        match request {
            FileRequest::Import => {
                self.start_file_job(|| {
                    let selection = super::host_dialogs::pick_flash_folder()
                        .map(|path| super::host_import::read_flash_directory(&path));
                    Ok(FileSelection::Import(selection))
                });
            }
            FileRequest::Archive => {
                self.start_file_job(|| {
                    Ok(FileSelection::Archive(
                        if confirm_flash_storage_archive_replacement() {
                            pick_flash_storage_archive()
                        } else {
                            None
                        },
                    ))
                });
            }
            FileRequest::Export(number) => match self.snapshot.export_names.get(&number).cloned() {
                Some(Ok(name)) => self.start_file_job(move || {
                    Ok(FileSelection::Export(number, save_flash_file(name)))
                }),
                error => {
                    let error = error
                        .and_then(Result::err)
                        .unwrap_or_else(|| format!("Flash Drive file {number} does not exist"));
                    self.file_notice = Some((true, format!("Flash Drive export failed: {error}")));
                    self.send(Command::Modal(false));
                }
            },
            FileRequest::ExportArchive => {
                if self.snapshot.directory.is_empty() {
                    self.file_notice = Some((
                        true,
                        "Flash Drive archive export failed: Flash Drive contains no files"
                            .to_owned(),
                    ));
                    self.send(Command::Modal(false));
                } else {
                    self.start_file_job(|| Ok(FileSelection::ExportArchive(save_flash_archive())));
                }
            }
            FileRequest::Screen(id) => {
                self.start_file_job(move || Ok(FileSelection::Screen(id, save_screen_copy())));
            }
        }
    }

    fn start_file_job(
        &mut self,
        job: impl FnOnce() -> Result<FileSelection, String> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::sync_channel(1);
        match thread::Builder::new()
            .name("tek4051-file-dialog".to_owned())
            .spawn(move || {
                let result = job();
                // Closing the native app can legitimately discard a completed dialog.
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.file_job = Some(receiver);
                self.file_notice = None;
            }
            Err(error) => {
                self.send(Command::Modal(false));
                if let Some(id) = self.screen_in_flight {
                    self.send(Command::CancelScreenCopy(id));
                }
                self.file_notice = Some((true, format!("Cannot start file dialog: {error}")));
            }
        }
    }

    fn poll_file_job(&mut self) {
        let Some(receiver) = &self.file_job else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("file dialog worker ended without a result".to_owned())
            }
        };
        self.file_job = None;
        match result {
            Ok(FileSelection::Import(Some(selection))) => {
                self.send(Command::ImportSelectedFiles(selection));
            }
            Ok(FileSelection::Archive(Some(path))) => self.send(Command::ImportArchive(path)),
            Ok(FileSelection::Export(number, Some(path))) => {
                self.send(Command::ExportFile { number, path })
            }
            Ok(FileSelection::ExportArchive(Some(path))) => self.send(Command::ExportArchive(path)),
            Ok(FileSelection::Screen(id, Some(path))) => {
                self.send(Command::SaveScreen { id, path })
            }
            Ok(FileSelection::Screen(id, None)) => self.send(Command::CancelScreenCopy(id)),
            Ok(_) => self.file_notice = Some((false, "File operation cancelled".to_owned())),
            Err(error) => {
                if let Some(id) = self.screen_in_flight {
                    self.send(Command::CancelScreenCopy(id));
                }
                self.file_notice = Some((true, error));
            }
        }
        self.send(Command::Modal(false));
    }
}

impl eframe::App for Tek4051App<WorkerBackend> {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_at(context, Instant::now());
        #[cfg(feature = "webgpu")]
        {
            let submission_error = self
                .webgpu_submissions
                .as_ref()
                .and_then(|submissions| submissions.submit_pending().err());
            if let Some(error) = submission_error {
                self.webgpu_submissions = None;
                self.webgpu_texture_id = None;
                self.webgpu_notice = Some(error.to_string());
            }
        }
    }
}

fn notice_label(ui: &mut egui::Ui, error: bool, notice: &str) {
    if error {
        ui.colored_label(egui::Color32::LIGHT_RED, notice);
    } else {
        ui.label(notice);
    }
}

fn workload_graph(
    ui: &mut egui::Ui,
    samples: &[WorkloadSample],
    sample_received_at: Instant,
    now: Instant,
) {
    let width = ui.available_width();
    let (response, painter) = ui.allocate_painter(egui::vec2(width, 88.0), egui::Sense::hover());
    let rect = response.rect;
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 42.0, rect.top() + 6.0),
        egui::pos2(rect.right() - 8.0, rect.bottom() - 14.0),
    );
    let upper = (samples
        .iter()
        .map(|sample| sample.workload_percent)
        .fold(100.0, f32::max)
        * 1.1)
        .max(120.0);
    let y_at = |value: f32| plot.bottom() - plot.height() * value / upper;
    let axis = egui::Stroke::new(1.0_f32, egui::Color32::DARK_GRAY);
    let labels = egui::FontId::proportional(10.0);

    painter.line_segment([plot.left_bottom(), plot.left_top()], axis);
    painter.line_segment([plot.left_bottom(), plot.right_bottom()], axis);
    for (value, color) in [
        (100.0, egui::Color32::LIGHT_RED),
        (upper, egui::Color32::DARK_GRAY),
    ] {
        let y = y_at(value);
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, color),
        );
        painter.text(
            egui::pos2(rect.left() + 38.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{value:.0}%"),
            labels.clone(),
            color,
        );
    }
    if samples.is_empty() {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "Waiting for worker samples",
            labels,
            egui::Color32::GRAY,
        );
        return;
    }
    let oldest_time = samples[0].elapsed_seconds;
    let latest_time = samples[samples.len() - 1].elapsed_seconds;
    let time_span = (latest_time - oldest_time).max(3.0);
    // Let the viewport advance between worker publications, then stop if updates stall.
    let elapsed_since_sample = now
        .saturating_duration_since(sample_received_at)
        .min(WORKLOAD_SMOOTHING_LIMIT)
        .as_secs_f64();
    let visible_right = latest_time + elapsed_since_sample;
    let visible_left = visible_right - time_span;
    {
        let mut tick = visible_left.ceil();
        while tick <= visible_right {
            let x = plot.left() + plot.width() * ((tick - visible_left) / time_span) as f32;
            painter.line_segment(
                [
                    egui::pos2(x, plot.bottom()),
                    egui::pos2(x, plot.bottom() + 8.0),
                ],
                egui::Stroke::new(1.5_f32, egui::Color32::GRAY),
            );
            tick += 1.0;
        }
    }
    let workload_stroke = egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_BLUE);
    let segments = workload_sample_segments(samples);
    for (index, segment) in segments.iter().enumerate() {
        let mut points: Vec<_> = segment
            .iter()
            .map(|sample| {
                let x = plot.left()
                    + plot.width() * ((sample.elapsed_seconds - visible_left) / time_span) as f32;
                egui::pos2(x, y_at(sample.workload_percent.clamp(0.0, upper)))
            })
            .collect();
        if index + 1 == segments.len()
            && let Some(last) = points.last().copied()
        {
            points.push(egui::pos2(plot.right(), last.y));
        }
        if points.len() == 1 {
            painter.circle_filled(points[0], 2.0, egui::Color32::LIGHT_BLUE);
        } else {
            painter.add(egui::Shape::line(points, workload_stroke));
        }
    }
}

#[cfg(test)]
#[path = "inline.rs"]
mod inline;
#[cfg(test)]
pub(super) use inline::InlineApp;

#[cfg(test)]
#[path = "gui_tests.rs"]
mod tests;
