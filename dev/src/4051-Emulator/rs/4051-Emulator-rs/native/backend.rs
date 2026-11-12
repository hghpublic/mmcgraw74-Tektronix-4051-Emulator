//! Native worker transport and off-repaint preparation of session snapshots.
//!
//! A single publication pairs neutral state with its green egui image. Both pixel
//! copying and color conversion precede the short lock that GUI polls never wait
//! for. A bounded workload series measures emulation advancement against each
//! 10 ms worker budget. Commands enter one bounded FIFO that the worker drains independently.
//! Queue submission never waits for worker capacity; overflow is reported explicitly.
//! Shutdown never joins from `Drop`.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use eframe::egui;
#[cfg(feature = "webgpu")]
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
#[cfg(feature = "webgpu")]
use tek4051_emulator::emulation::webgpu::WebGpuVectorRenderer;
use tek4051_emulator::{
    emulation::{Command, CpuReferenceRenderer, EmulationState, Snapshot, TICK},
    emulation_input::{InputAction, InputFrame},
    machine::Machine,
};

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "backend/inline.rs"]
mod inline;
#[cfg(test)]
pub(super) use inline::InlineBackend;

const COMMAND_BATCH_CAPACITY: usize = 64;
const MAX_PENDING_KEY_TRANSITIONS: usize = 1024;
const MAX_PENDING_COMMANDS: usize = 1024;
const PENDING_CAPACITY: usize = MAX_PENDING_COMMANDS + 1;
const WORKLOAD_HISTORY_CAPACITY: usize = 300;

fn pending_key_transitions(command: &Command) -> usize {
    match command {
        Command::Input(frame) => frame
            .actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    InputAction::ManualPress { .. }
                        | InputAction::ManualRelease(_)
                        | InputAction::SequencedManualPress { .. }
                        | InputAction::SequencedManualRelease(_)
                        | InputAction::PanelPress { .. }
                        | InputAction::PanelRelease(_)
                )
            })
            .count(),
        Command::AutoLoad => 2 * b"FIND@5:1\rOLD@5:\rRUN\r".len(),
        Command::Launch(number) => {
            // A selected program may be binary, so reserve for its longer loader command.
            2 * format!("FIND@5:{number}\rCALL \"BOLD\",5\rRUN\r").len()
        }
        _ => 0,
    }
}

fn total_pending_key_transitions(queue: &CommandQueue) -> usize {
    queue.pending_key_transitions
        + queue.processing_key_transitions
        + queue.sequenced_key_transitions
}

/// State and its prepared image travel together, including the initial publication.
#[derive(Clone)]
pub(super) struct PresentedSnapshot {
    pub(super) snapshot: Arc<Snapshot>,
    pub(super) image: Arc<egui::ColorImage>,
    /// Recent `advance_realtime` durations and their worker-elapsed timestamps.
    pub(super) workload_samples: Arc<[WorkloadSample]>,
}

/// Per-backend caching prevents metadata-only publications from recoloring pixels.
#[derive(Default)]
struct DisplayPresentation {
    cached_image: Option<(u64, Arc<egui::ColorImage>)>,
}

impl DisplayPresentation {
    fn prepare(&mut self, snapshot: Arc<Snapshot>) -> PresentedSnapshot {
        let image = match &self.cached_image {
            Some((_, image)) if snapshot.webgpu_active => Arc::clone(image),
            Some((revision, image)) if *revision == snapshot.revision => Arc::clone(image),
            _ => {
                let image = Arc::new(egui::ColorImage::new(
                    [CpuReferenceRenderer::WIDTH, CpuReferenceRenderer::HEIGHT],
                    snapshot
                        .pixels
                        .iter()
                        .map(|&pixel| egui::Color32::from_rgb(0, pixel, 0))
                        .collect(),
                ));
                self.cached_image = Some((snapshot.revision, Arc::clone(&image)));
                image
            }
        };
        PresentedSnapshot {
            snapshot,
            image,
            workload_samples: Arc::from([]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WorkloadSample {
    pub(super) elapsed_seconds: f64,
    pub(super) workload_percent: f32,
}

/// Retains recent emulation-advance costs for the native graph and speed probes.
struct WorkerWorkload {
    started_at: Instant,
    samples: VecDeque<WorkloadSample>,
}

impl WorkerWorkload {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            samples: VecDeque::new(),
        }
    }

    fn record(&mut self, elapsed: std::time::Duration) {
        if self.samples.len() == WORKLOAD_HISTORY_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(WorkloadSample {
            elapsed_seconds: self.started_at.elapsed().as_secs_f64(),
            workload_percent: (elapsed.as_secs_f64() / TICK.as_secs_f64() * 100.0) as f32,
        });
    }

    fn clear(&mut self) {
        self.samples.clear();
    }

    fn snapshot(&self) -> Arc<[WorkloadSample]> {
        self.samples.iter().copied().collect::<Vec<_>>().into()
    }
}

fn clear_workload_if_speed_command(command: &Command, workload: &mut WorkerWorkload) {
    if matches!(command, Command::Speed(_)) {
        workload.clear();
    }
}

pub(super) trait Backend {
    fn command(&mut self, command: Command) -> Result<(), String>;
    fn snapshot(&mut self) -> Result<Option<PresentedSnapshot>, String>;
    fn advance(&mut self, now: Instant);
    fn is_connected(&self) -> bool {
        true
    }

    #[cfg(feature = "webgpu")]
    fn enable_webgpu_renderer(&mut self, _renderer: WebGpuVectorRenderer) -> Result<(), String> {
        Err("this native backend does not accept a WebGPU renderer".to_owned())
    }
}

struct Shared {
    latest: Mutex<Option<PresentedSnapshot>>,
    commands: Mutex<CommandQueue>,
    shutdown: AtomicBool,
    finished: AtomicBool,
}

#[derive(Default)]
struct CommandQueue {
    pending: VecDeque<Command>,
    pending_key_transitions: usize,
    processing_key_transitions: usize,
    sequenced_key_transitions: usize,
    overflow: Option<String>,
}

struct FinishSignal(Arc<Shared>);

impl Drop for FinishSignal {
    fn drop(&mut self) {
        self.0.finished.store(true, Ordering::Release);
    }
}

/// A cloneable shutdown signal can outlive the app moved into `run_native`.
#[derive(Clone)]
pub(super) struct ShutdownHandle(Arc<Shared>);

impl ShutdownHandle {
    pub(super) fn request_shutdown(&self) {
        self.0.shutdown.store(true, Ordering::Release);
    }
}

pub(super) struct WorkerBackend {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<Result<(), String>>>,
    failure: Option<String>,
    initial: Option<PresentedSnapshot>,
    #[cfg(feature = "webgpu")]
    webgpu_sender: SyncSender<WebGpuVectorRenderer>,
}

impl WorkerBackend {
    pub(super) fn start(machine: Machine) -> Result<Self, String> {
        #[cfg(feature = "webgpu")]
        let (webgpu_sender, webgpu_receiver) = mpsc::sync_channel(1);
        // Establish the initial paused view before transferring ownership to the worker.
        // Construction therefore needs no waiting/retry loop in the native app factory.
        let mut state = EmulationState::new(machine);
        let mut presentation = DisplayPresentation::default();
        let initial = presentation.prepare(state.snapshot());
        let shared = Arc::new(Shared {
            latest: Mutex::new(Some(initial.clone())),
            commands: Mutex::new(CommandQueue::default()),
            shutdown: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("tek4051-emulation".to_owned())
            .spawn(move || {
                // Also marks termination when unwinding, even if the GUI transferred
                // its JoinHandle to an owner outside the native event loop.
                let _finished = FinishSignal(Arc::clone(&worker_shared));
                worker_loop(
                    state,
                    presentation,
                    &worker_shared,
                    #[cfg(feature = "webgpu")]
                    webgpu_receiver,
                )
            })
            .map_err(|error| format!("cannot start emulation worker: {error}"))?;
        Ok(Self {
            shared,
            worker: Some(worker),
            failure: None,
            initial: Some(initial),
            #[cfg(feature = "webgpu")]
            webgpu_sender,
        })
    }

    pub(super) fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle(Arc::clone(&self.shared))
    }

    /// The caller may join this handle only after the native event loop returns.
    pub(super) fn take_join_handle(&mut self) -> Option<JoinHandle<Result<(), String>>> {
        self.worker.take()
    }

    pub(super) fn request_shutdown(&self) {
        self.shared.shutdown.store(true, Ordering::Release);
    }

    fn fail(&mut self, message: &str) -> String {
        self.failure = Some(message.to_owned());
        self.request_shutdown();
        message.to_owned()
    }

    fn ensure_worker_available(&mut self) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.shared.finished.load(Ordering::Acquire)
            || self.shared.shutdown.load(Ordering::Acquire)
            || self.worker.as_ref().is_some_and(JoinHandle::is_finished)
        {
            return Err(self.fail("emulation worker disconnected or shut down"));
        }
        Ok(())
    }
}

impl Backend for WorkerBackend {
    fn command(&mut self, command: Command) -> Result<(), String> {
        self.ensure_worker_available()?;
        let mut queue = self
            .shared
            .commands
            .lock()
            .map_err(|_| "host command queue lock poisoned".to_owned())?;
        if matches!(&command, Command::Input(_)) {
            if let Some(error) = &queue.overflow {
                return Err(error.clone());
            }
        }
        if let (Some(Command::Input(previous)), Command::Input(next)) =
            (queue.pending.back(), &command)
            && previous.actions.is_empty()
            && next.actions.is_empty()
            && previous.focused == next.focused
            && (!next.focused
                || (previous.modifiers == next.modifiers
                    && previous.reset_input == next.reset_input))
        {
            return Ok(());
        }
        let key_transitions = pending_key_transitions(&command);
        if total_pending_key_transitions(&queue).saturating_add(key_transitions)
            > MAX_PENDING_KEY_TRANSITIONS
        {
            let error = format!(
                "keyboard input queue full ({MAX_PENDING_KEY_TRANSITIONS} pending key transitions); newest input frame was not queued"
            );
            queue.overflow = Some(error.clone());
            queue
                .pending
                .push_back(Command::Input(Box::new(InputFrame::unfocused())));
            return Err(error);
        }
        if queue.overflow.is_none() && queue.pending.len() >= MAX_PENDING_COMMANDS {
            let error = format!(
                "host command queue full ({MAX_PENDING_COMMANDS} pending commands); newest command was not queued"
            );
            queue.overflow = Some(error.clone());
            queue
                .pending
                .push_back(Command::Input(Box::new(InputFrame::unfocused())));
            return Err(error);
        }
        if queue.pending.len() >= PENDING_CAPACITY {
            let error = queue.overflow.clone().unwrap_or_else(|| {
                format!(
                    "host command queue full ({MAX_PENDING_COMMANDS} pending commands); newest command was not queued"
                )
            });
            return Err(error);
        }
        queue.pending_key_transitions += key_transitions;
        queue.pending.push_back(command);
        Ok(())
    }

    fn snapshot(&mut self) -> Result<Option<PresentedSnapshot>, String> {
        self.ensure_worker_available()?;
        if let Some(initial) = self.initial.take() {
            return Ok(Some(initial));
        }
        match self.shared.latest.try_lock() {
            Ok(latest) => Ok(latest.clone()),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(_)) => Err("emulation snapshot lock poisoned".to_owned()),
        }
    }

    fn advance(&mut self, _now: Instant) {}

    fn is_connected(&self) -> bool {
        self.failure.is_none()
            && !self.shared.finished.load(Ordering::Acquire)
            && !self.shared.shutdown.load(Ordering::Acquire)
            && self
                .worker
                .as_ref()
                .is_none_or(|worker| !worker.is_finished())
    }

    #[cfg(feature = "webgpu")]
    fn enable_webgpu_renderer(&mut self, renderer: WebGpuVectorRenderer) -> Result<(), String> {
        self.webgpu_sender
            .send(renderer)
            .map_err(|error| format!("cannot send WebGPU renderer to emulation worker: {error}"))
    }
}

impl Drop for WorkerBackend {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn publish(
    state: &mut EmulationState,
    presentation: &mut DisplayPresentation,
    workload: &WorkerWorkload,
    shared: &Shared,
) -> Result<(), String> {
    // Snapshot creation and native color conversion both precede locking.
    let mut snapshot = presentation.prepare(state.snapshot());
    snapshot.workload_samples = workload.snapshot();
    *shared
        .latest
        .lock()
        .map_err(|_| "emulation snapshot lock poisoned".to_owned())? = Some(snapshot);
    Ok(())
}

fn take_worker_command(shared: &Shared) -> Result<Option<Command>, String> {
    let mut queue = shared
        .commands
        .lock()
        .map_err(|_| "host command queue lock poisoned".to_owned())?;
    let Some(command) = queue.pending.pop_front() else {
        return Ok(None);
    };
    let transitions = pending_key_transitions(&command);
    queue.pending_key_transitions -= transitions;
    queue.processing_key_transitions += transitions;
    Ok(Some(command))
}

fn complete_worker_command(
    shared: &Shared,
    state: &EmulationState,
    transitions: usize,
) -> Result<(), String> {
    let mut queue = shared
        .commands
        .lock()
        .map_err(|_| "host command queue lock poisoned".to_owned())?;
    queue.processing_key_transitions = queue
        .processing_key_transitions
        .checked_sub(transitions)
        .ok_or_else(|| "worker input-transition accounting underflow".to_owned())?;
    queue.sequenced_key_transitions = state.pending_input_transitions();
    Ok(())
}

fn sync_sequenced_key_transitions(shared: &Shared, state: &EmulationState) -> Result<(), String> {
    let mut queue = shared
        .commands
        .lock()
        .map_err(|_| "host command queue lock poisoned".to_owned())?;
    queue.sequenced_key_transitions = state.pending_input_transitions();
    Ok(())
}

fn worker_loop(
    mut state: EmulationState,
    mut presentation: DisplayPresentation,
    shared: &Shared,
    #[cfg(feature = "webgpu")] webgpu_receiver: Receiver<WebGpuVectorRenderer>,
) -> Result<(), String> {
    let mut workload = WorkerWorkload::new();
    publish(&mut state, &mut presentation, &workload, shared)?;
    let result = loop {
        if shared.shutdown.load(Ordering::Acquire) {
            break Ok(());
        }
        #[cfg(feature = "webgpu")]
        match webgpu_receiver.try_recv() {
            Ok(renderer) => {
                if let Err(error) = state.enable_webgpu_renderer(renderer) {
                    eprintln!("WebGPU renderer activation failed: {error}");
                }
                publish(&mut state, &mut presentation, &workload, shared)?;
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
        }
        // A bounded batch lets shutdown and time advancement run even during heavy input.
        let mut command_queue_error = None;
        for _ in 0..COMMAND_BATCH_CAPACITY {
            if shared.shutdown.load(Ordering::Acquire) {
                break;
            }
            let command = match take_worker_command(shared) {
                Ok(Some(command)) => command,
                Ok(None) => break,
                Err(error) => {
                    command_queue_error = Some(error);
                    break;
                }
            };
            let key_transitions = pending_key_transitions(&command);
            clear_workload_if_speed_command(&command, &mut workload);
            if matches!(command, Command::BeginFileOperation(_)) {
                apply_worker_command(&mut state, command, Instant::now())?;
                complete_worker_command(shared, &state, key_transitions)?;
                publish(&mut state, &mut presentation, &workload, shared)?;
                continue;
            }
            let file_operation = matches!(
                command,
                Command::ImportFiles(_)
                    | Command::ImportSelectedFiles(_)
                    | Command::ImportArchive(_)
                    | Command::ExportFile { .. }
                    | Command::ExportArchive(_)
                    | Command::SaveScreen { .. }
            );
            let was_modal = if file_operation {
                let was_modal = state.begin_file_io();
                publish(&mut state, &mut presentation, &workload, shared)?;
                Some(was_modal)
            } else {
                None
            };
            if let Err(error) = apply_worker_command(&mut state, command, Instant::now()) {
                state.fail(error);
            }
            complete_worker_command(shared, &state, key_transitions)?;
            if let Some(was_modal) = was_modal {
                // Account for blocking file time before a queued Modal(false) can
                // resume interval work; erase callbacks still use that wall time.
                state.finish_file_io(Instant::now(), was_modal);
                sync_sequenced_key_transitions(shared, &state)?;
                publish(&mut state, &mut presentation, &workload, shared)?;
            }
        }
        if let Some(error) = command_queue_error {
            break Err(error);
        }
        if shared.shutdown.load(Ordering::Acquire) {
            break Ok(());
        }
        let now = Instant::now();
        let work_started = Instant::now();
        state.advance_realtime(now);
        workload.record(work_started.elapsed());
        sync_sequenced_key_transitions(shared, &state)?;
        if let Err(error) = publish(&mut state, &mut presentation, &workload, shared) {
            break Err(error);
        }
        // Expensive ticks consume this interval rather than creating catch-up debt.
        thread::sleep(TICK.saturating_sub(now.elapsed()));
    };
    state.stop();
    let final_publication = publish(&mut state, &mut presentation, &workload, shared);
    result.and(final_publication)
}

fn apply_worker_command(
    state: &mut EmulationState,
    command: Command,
    now: Instant,
) -> Result<(), String> {
    if matches!(
        &command,
        Command::Run(_)
            | Command::Input(_)
            | Command::AutoLoad
            | Command::Launch(_)
            | Command::Modal(_)
            | Command::BeginFileOperation(_)
    ) {
        state.advance_realtime(now);
    }
    apply_command(state, command)
}

/// Complete any host effect synchronously on this owner before draining more commands.
fn apply_command(state: &mut EmulationState, command: Command) -> Result<(), String> {
    if let Some(effect) = state.apply(command)? {
        let outcome = super::host_io::execute(effect, state);
        state.complete_file_effect(outcome);
    }
    Ok(())
}
