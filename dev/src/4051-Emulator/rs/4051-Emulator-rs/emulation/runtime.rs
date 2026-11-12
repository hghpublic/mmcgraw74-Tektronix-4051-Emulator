//! Elapsed-time scheduling for the Tektronix 4051 machine.
//!
//! The web reference runs CPU execution and keyboard refresh callbacks every
//! 10 ms, display phosphor updates every 100 ms, and signals display readiness
//! 500 ms after an erase. This runtime applies those intervals from caller-
//! supplied elapsed time without adding clocks or delays to `Machine`.

use std::{collections::VecDeque, error::Error, fmt, time::Duration};

use super::display::{CpuReferenceRenderer, DisplayEventBatch, DisplayGranularity};
#[cfg(feature = "webgpu")]
use super::display::{DisplayEvent, DisplayPoint, MAX_PENDING_DISPLAY_EVENTS};
#[cfg(feature = "webgpu")]
use super::extended_webgpu::{ExtendedWebGpuRenderer, ExtendedWebGpuRendererError};
#[cfg(feature = "webgpu")]
use super::webgpu::WebGpuVectorRenderer;
#[cfg(feature = "webgpu")]
use crate::machine::display::DisplayOperation;
use crate::machine::{Machine, MachineBusError, cpu::CpuError};

#[cfg(all(test, feature = "webgpu", not(target_arch = "wasm32")))]
#[path = "runtime_tests.rs"]
mod tests;

pub(crate) const EXECUTION_AND_KEYBOARD_INTERVAL: Duration = Duration::from_millis(10);
const DISPLAY_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
pub const DISPLAY_ERASE_DELAY: Duration = Duration::from_millis(500);
const REFERENCE_MIPS: f64 = 0.833;
const REFERENCE_SPEED_DIVISOR: f64 = 368.0;
const DEFAULT_INSTRUCTIONS_PER_INTERVAL: usize = 2_264;
#[cfg(feature = "webgpu")]
const MAX_PENDING_SCREEN_COPIES: usize = 8;

/// Events the host must apply to presentation after advancing emulation time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeEvents {
    /// Number of 100 ms phosphor-update callbacks due during this advance.
    pub display_refreshes: u128,
}

/// An invalid speed factor or one whose instruction budget cannot fit `usize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedFactorError {
    NotFiniteAndPositive,
    InstructionBudgetOverflow,
}

impl fmt::Display for SpeedFactorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFiniteAndPositive => {
                write!(
                    formatter,
                    "speed factor must be finite and greater than zero"
                )
            }
            Self::InstructionBudgetOverflow => {
                write!(
                    formatter,
                    "speed factor produces an unrepresentable instruction budget"
                )
            }
        }
    }
}

impl Error for SpeedFactorError {}

/// An execution failure stops the runtime; the host must reset and start it again.
#[derive(Debug, PartialEq, Eq)]
pub enum HostRuntimeError {
    Machine {
        source: CpuError<MachineBusError>,
        events: RuntimeEvents,
    },
    Stopped,
}

impl fmt::Display for HostRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Machine { source, .. } => {
                write!(formatter, "machine execution failed: {source}")
            }
            Self::Stopped => write!(formatter, "host runtime is stopped after a machine error"),
        }
    }
}

impl Error for HostRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Machine { source, .. } => Some(source),
            Self::Stopped => None,
        }
    }
}

/// Schedules deterministic machine work against elapsed time supplied by a host.
///
/// CPU and keyboard callbacks share the reference's 10 ms cadence. Display
/// refreshes update the machine's display model at the reference cadence, while
/// each erase request gets an independent completion after the reference delay.
/// Pausing freezes CPU and keyboard callbacks, but erase timeouts and Extended
/// WebGPU phosphor decay keep advancing with host-supplied elapsed time.
pub struct HostRuntime {
    execution_elapsed: Duration,
    display_elapsed: Duration,
    erase_completions: VecDeque<Duration>,
    instructions_per_interval: usize,
    renderer: CpuReferenceRenderer,
    #[cfg(feature = "webgpu")]
    speed_factor: f64,
    #[cfg(feature = "webgpu")]
    webgpu_renderer: Option<WebGpuVectorRenderer>,
    #[cfg(feature = "webgpu")]
    extended_webgpu_renderer: Option<ExtendedWebGpuRenderer>,
    #[cfg(feature = "webgpu")]
    render_error: Option<String>,
    #[cfg(feature = "webgpu")]
    presentation_revision: u64,
    #[cfg(feature = "webgpu")]
    pending_display_events: VecDeque<DisplayEvent>,
    #[cfg(feature = "webgpu")]
    pending_cursor_marks: std::collections::HashSet<DisplayPoint>,
    #[cfg(feature = "webgpu")]
    display_events_dropped: bool,
    #[cfg(feature = "webgpu")]
    screen_copies: VecDeque<Vec<u8>>,
    #[cfg(feature = "webgpu")]
    dropped_screen_copies: usize,
    #[cfg(feature = "webgpu")]
    screen_copy_warnings: VecDeque<String>,
    paused: bool,
    stopped: bool,
}

impl HostRuntime {
    /// Starts host timing and signals display readiness unless an erase is pending.
    ///
    /// The caller remains responsible for resetting `machine` before starting it.
    pub fn start(machine: &mut Machine) -> Self {
        let mut runtime = Self {
            execution_elapsed: Duration::ZERO,
            display_elapsed: Duration::ZERO,
            erase_completions: VecDeque::new(),
            instructions_per_interval: DEFAULT_INSTRUCTIONS_PER_INTERVAL,
            renderer: CpuReferenceRenderer::default(),
            #[cfg(feature = "webgpu")]
            speed_factor: 1.0,
            #[cfg(feature = "webgpu")]
            webgpu_renderer: None,
            #[cfg(feature = "webgpu")]
            extended_webgpu_renderer: None,
            #[cfg(feature = "webgpu")]
            render_error: None,
            #[cfg(feature = "webgpu")]
            presentation_revision: 0,
            #[cfg(feature = "webgpu")]
            pending_display_events: VecDeque::new(),
            #[cfg(feature = "webgpu")]
            pending_cursor_marks: std::collections::HashSet::new(),
            #[cfg(feature = "webgpu")]
            display_events_dropped: false,
            #[cfg(feature = "webgpu")]
            screen_copies: VecDeque::new(),
            #[cfg(feature = "webgpu")]
            dropped_screen_copies: 0,
            #[cfg(feature = "webgpu")]
            screen_copy_warnings: VecDeque::new(),
            paused: false,
            stopped: false,
        };
        if !machine.display_erase_pending() {
            machine.display_ready();
        }

        runtime.sync_display(machine);
        runtime.sync_erase_requests(machine);
        runtime
    }

    /// Restarts host timing after the caller resets the machine.
    ///
    /// The configured speed and existing erase timeouts are retained. New erase
    /// requests are scheduled without restarting earlier completion delays.
    pub fn restart(&mut self, machine: &mut Machine) {
        self.reset_elapsed();
        self.paused = false;
        self.stopped = false;
        if !machine.display_erase_pending() {
            machine.display_ready();
        }
        self.sync_display(machine);
        self.sync_erase_requests(machine);
    }

    /// Applies queued machine operations before returning the current rendered display.
    pub fn display(&mut self, machine: &mut Machine) -> &CpuReferenceRenderer {
        self.sync_display(machine);
        &self.renderer
    }

    /// Changes the reference renderer's dot sizing mode.
    pub fn set_display_granularity(
        &mut self,
        machine: &mut Machine,
        granularity: DisplayGranularity,
    ) {
        self.sync_display(machine);
        self.renderer.set_granularity(granularity);
    }

    /// Drains screen-copy snapshots produced by machine display operations.
    pub fn take_screen_copies(&mut self, machine: &mut Machine) -> (Vec<Vec<u8>>, usize) {
        self.sync_display(machine);
        #[cfg(feature = "webgpu")]
        {
            (
                self.screen_copies.drain(..).collect(),
                std::mem::take(&mut self.dropped_screen_copies),
            )
        }
        #[cfg(not(feature = "webgpu"))]
        self.renderer.take_screen_copies()
    }

    pub(crate) fn take_display_events(&mut self) -> DisplayEventBatch {
        #[cfg(feature = "webgpu")]
        {
            let events = self.pending_display_events.drain(..).collect();
            self.pending_cursor_marks.clear();
            DisplayEventBatch {
                events,
                dropped: std::mem::take(&mut self.display_events_dropped),
            }
        }
        #[cfg(not(feature = "webgpu"))]
        self.renderer.take_events()
    }

    pub(crate) fn clear_display_events(&mut self) {
        self.renderer.clear_events();
        #[cfg(feature = "webgpu")]
        {
            self.pending_display_events.clear();
            self.pending_cursor_marks.clear();
            self.display_events_dropped = false;
        }
    }

    fn sync_display(&mut self, machine: &mut Machine) {
        #[cfg(feature = "webgpu")]
        {
            // Preserve operation order while submitting only one GPU frame per machine batch.
            for operation in machine.take_display_operations() {
                self.apply_display_operation(operation);
            }
            if let Some(renderer) = self.webgpu_renderer.as_mut()
                && let Err(error) = renderer.finish_display_batch()
            {
                self.webgpu_renderer = None;
                self.render_error = Some(format!(
                    "WebGPU rendering failed; using the CPU reference renderer: {error}"
                ));
                self.presentation_revision = self.presentation_revision.wrapping_add(1);
            }
            let extended_revision = self
                .extended_webgpu_renderer
                .as_ref()
                .map(ExtendedWebGpuRenderer::revision);
            if let Some(renderer) = self.extended_webgpu_renderer.as_mut()
                && let Err(error) = renderer
                    .finish_display_batch()
                    .and_then(|()| renderer.compose_frame())
            {
                self.extended_webgpu_renderer = None;
                self.render_error = Some(format!(
                    "Extended WebGPU rendering failed; using the CPU reference renderer: {error}"
                ));
                self.presentation_revision = self.presentation_revision.wrapping_add(1);
            } else if self
                .extended_webgpu_renderer
                .as_ref()
                .is_some_and(|renderer| Some(renderer.revision()) != extended_revision)
            {
                self.presentation_revision = self.presentation_revision.wrapping_add(1);
            }
        }
        #[cfg(not(feature = "webgpu"))]
        self.renderer
            .apply_operations(machine.take_display_operations());
    }

    #[cfg(feature = "webgpu")]
    fn apply_display_operation(&mut self, operation: DisplayOperation) {
        let revision_before = self.renderer.revision();
        self.renderer.apply_operations([operation]);
        let events = self.renderer.take_events();
        let gpu_was_active = self.webgpu_renderer.is_some();
        let extended_was_active = self.extended_webgpu_renderer.is_some();
        let extended_revision = self
            .extended_webgpu_renderer
            .as_ref()
            .map(ExtendedWebGpuRenderer::revision);
        let gpu_error = self.webgpu_renderer.as_mut().and_then(|renderer| {
            let batch = DisplayEventBatch {
                events: events.events.clone(),
                dropped: events.dropped,
            };
            renderer
                .apply_display_operation(operation, batch, self.renderer.granularity())
                .err()
                .map(|error| error.to_string())
        });
        let extended_error = self.extended_webgpu_renderer.as_mut().and_then(|renderer| {
            let batch = DisplayEventBatch {
                events: events.events.clone(),
                dropped: events.dropped,
            };
            renderer
                .apply_display_operation(operation, batch, self.renderer.granularity())
                .err()
                .map(|error| error.to_string())
        });
        self.queue_display_events(events);

        if matches!(operation, DisplayOperation::ScreenCopy) {
            let (cpu_copies, cpu_dropped) = self.renderer.take_screen_copies();
            if gpu_was_active {
                if let Some(error) = &gpu_error {
                    self.screen_copy_warnings.push_back(format!(
                        "WebGPU screen copy failed; using CPU reference pixels: {error}"
                    ));
                    self.queue_screen_copies(cpu_copies, cpu_dropped);
                } else if let Some(renderer) = self.webgpu_renderer.as_mut() {
                    let (copies, dropped) = renderer.take_screen_copies();
                    self.queue_screen_copies(copies, dropped);
                }
            } else {
                if extended_was_active {
                    if let Some(error) = &extended_error {
                        self.screen_copy_warnings.push_back(format!(
                            "Extended WebGPU screen copy failed; using CPU reference pixels: {error}"
                        ));
                        self.queue_screen_copies(cpu_copies, cpu_dropped);
                    } else if cfg!(target_arch = "wasm32") {
                        self.screen_copy_warnings.push_back(
                            "Extended WebGPU screen copy uses CPU reference pixels in the browser"
                                .to_owned(),
                        );
                        self.queue_screen_copies(cpu_copies, cpu_dropped);
                    } else if let Some(renderer) = self.extended_webgpu_renderer.as_mut() {
                        let (copies, dropped) = renderer.take_screen_copies();
                        self.queue_screen_copies(copies, dropped);
                    }
                } else {
                    self.queue_screen_copies(cpu_copies, cpu_dropped);
                }
            }
        }

        if let Some(error) = gpu_error {
            self.webgpu_renderer = None;
            self.render_error = Some(format!(
                "WebGPU rendering failed; using the CPU reference renderer: {error}"
            ));
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
        if let Some(error) = extended_error {
            self.extended_webgpu_renderer = None;
            self.render_error = Some(format!(
                "Extended WebGPU rendering failed; using the CPU reference renderer: {error}"
            ));
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        } else if self
            .extended_webgpu_renderer
            .as_ref()
            .is_some_and(|renderer| Some(renderer.revision()) != extended_revision)
        {
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }

        if self.renderer.revision() != revision_before {
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
    }

    #[cfg(feature = "webgpu")]
    fn queue_display_events(&mut self, batch: DisplayEventBatch) {
        if batch.dropped {
            self.pending_display_events.clear();
            self.pending_cursor_marks.clear();
            self.display_events_dropped = true;
            return;
        }
        if self.display_events_dropped {
            return;
        }
        for event in batch.events {
            match event {
                DisplayEvent::TransientCursorMark(position)
                    if !self.pending_cursor_marks.insert(position) =>
                {
                    continue;
                }
                DisplayEvent::TransientCursorMark(_) => {}
                _ => self.pending_cursor_marks.clear(),
            }
            if self.pending_display_events.len() == MAX_PENDING_DISPLAY_EVENTS {
                self.pending_display_events.clear();
                self.pending_cursor_marks.clear();
                self.display_events_dropped = true;
                return;
            }
            self.pending_display_events.push_back(event);
        }
    }

    #[cfg(feature = "webgpu")]
    fn queue_screen_copies(&mut self, copies: Vec<Vec<u8>>, dropped: usize) {
        for pixels in copies {
            if self.screen_copies.len() == MAX_PENDING_SCREEN_COPIES {
                self.dropped_screen_copies = self.dropped_screen_copies.saturating_add(1);
            } else {
                self.screen_copies.push_back(pixels);
            }
        }
        self.dropped_screen_copies = self.dropped_screen_copies.saturating_add(dropped);
    }

    #[cfg(feature = "webgpu")]
    pub fn enable_webgpu_renderer(
        &mut self,
        machine: &mut Machine,
        renderer: WebGpuVectorRenderer,
    ) -> Result<(), String> {
        self.sync_display(machine);
        if self.renderer.pixels().iter().any(|pixel| *pixel != 0) {
            return Err(
                "WebGPU renderer must be enabled before the display contains pixels".to_owned(),
            );
        }
        self.webgpu_renderer = Some(renderer);
        self.extended_webgpu_renderer = None;
        self.render_error = None;
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        Ok(())
    }

    #[cfg(feature = "webgpu")]
    pub fn enable_extended_webgpu_renderer(
        &mut self,
        machine: &mut Machine,
        renderer: ExtendedWebGpuRenderer,
    ) -> Result<(), String> {
        self.sync_display(machine);
        if self.renderer.pixels().iter().any(|pixel| *pixel != 0) {
            return Err(
                "Extended WebGPU renderer must be enabled before the display contains pixels"
                    .to_owned(),
            );
        }
        self.webgpu_renderer = None;
        let mut renderer = renderer;
        renderer.set_speed_factor(self.speed_factor);
        self.extended_webgpu_renderer = Some(renderer);
        self.render_error = None;
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        Ok(())
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn record_webgpu_error(&mut self, error: &str) {
        self.webgpu_renderer = None;
        self.extended_webgpu_renderer = None;
        self.render_error = Some(format!(
            "WebGPU renderer unavailable; using the CPU reference renderer: {error}"
        ));
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
    }

    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub(crate) fn record_webgpu_failure(&mut self, error: &str) {
        self.webgpu_renderer = None;
        self.extended_webgpu_renderer = None;
        self.render_error = Some(format!(
            "WebGPU presentation failed; using the CPU reference renderer: {error}"
        ));
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn webgpu_active(&self) -> bool {
        self.webgpu_renderer.is_some() || self.extended_webgpu_renderer.is_some()
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn extended_webgpu_active(&self) -> bool {
        self.extended_webgpu_renderer.is_some()
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn set_vector_scene_capture_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let Some(renderer) = self.extended_webgpu_renderer.as_mut() else {
            return if enabled {
                Err("Vector scene capture requires Extended WebGPU".to_owned())
            } else {
                Ok(())
            };
        };
        renderer.set_vector_scene_capture_enabled(enabled);
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        Ok(())
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn set_vector_zoom_mode(&mut self, enabled: bool) -> Result<(), String> {
        let Some(renderer) = self.extended_webgpu_renderer.as_mut() else {
            return Err("Vector zoom requires Extended WebGPU".to_owned());
        };
        if let Err(error) = renderer.set_vector_zoom_mode(enabled) {
            let message = error.to_string();
            if !matches!(
                error,
                ExtendedWebGpuRendererError::VectorSceneCaptureDisabled
                    | ExtendedWebGpuRendererError::InvalidVectorZoomCamera
            ) {
                self.record_webgpu_error(&message);
            }
            return Err(message);
        }
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        Ok(())
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn set_vector_zoom_camera(
        &mut self,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
    ) -> Result<(), String> {
        let Some(renderer) = self.extended_webgpu_renderer.as_mut() else {
            return Err("Vector zoom requires Extended WebGPU".to_owned());
        };
        if let Err(error) = renderer.set_vector_zoom_camera(zoom, pan_x, pan_y) {
            let message = error.to_string();
            if !matches!(
                error,
                ExtendedWebGpuRendererError::VectorSceneCaptureDisabled
                    | ExtendedWebGpuRendererError::InvalidVectorZoomCamera
            ) {
                self.record_webgpu_error(&message);
            }
            return Err(message);
        }
        if renderer.vector_zoom_active() {
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
        Ok(())
    }

    #[cfg(all(feature = "webgpu", target_arch = "wasm32"))]
    pub(crate) fn vector_zoom_presentation_view(&self) -> Option<&wgpu::TextureView> {
        self.extended_webgpu_renderer
            .as_ref()
            .map(ExtendedWebGpuRenderer::presentation_view)
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn vector_scene_capture_enabled(&self) -> bool {
        self.extended_webgpu_renderer
            .as_ref()
            .is_some_and(|renderer| renderer.vector_scene_memory_usage().is_some())
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn vector_scene_memory_usage(&self) -> Option<(usize, usize)> {
        self.extended_webgpu_renderer
            .as_ref()
            .and_then(ExtendedWebGpuRenderer::vector_scene_memory_usage)
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn render_error(&self) -> Option<&str> {
        self.render_error.as_deref()
    }

    #[cfg(feature = "webgpu")]
    pub(crate) fn take_screen_copy_warnings(&mut self) -> Vec<String> {
        self.screen_copy_warnings.drain(..).collect()
    }

    #[cfg(feature = "webgpu")]
    pub(crate) const fn presentation_revision(&self) -> u64 {
        self.presentation_revision
    }

    /// Pauses execution and restarts callback timing when resumed.
    ///
    /// CPU and keyboard callbacks ignore paused time. Erase timeouts and
    /// Extended WebGPU phosphor decay continue when the host advances time.
    /// Pausing an already paused or stopped runtime has no effect.
    pub fn pause(&mut self) {
        if self.stopped || self.paused {
            return;
        }

        self.reset_elapsed();
        self.paused = true;
    }

    /// Resumes a paused runtime, rejecting runtimes stopped by machine errors.
    pub fn resume(&mut self) -> Result<(), HostRuntimeError> {
        if self.stopped {
            return Err(HostRuntimeError::Stopped);
        }

        if self.paused {
            self.reset_elapsed();
            self.paused = false;
        }
        Ok(())
    }

    /// Sets the reference speed factor used to calculate each 10 ms CPU budget.
    ///
    /// As in the web CPU, a fractional instruction count executes the next whole
    /// instruction because the reference loop compares an integer counter with
    /// the floating-point budget. Invalid values leave the current budget intact.
    pub fn set_speed_factor(&mut self, speed_factor: f64) -> Result<(), SpeedFactorError> {
        let instructions_per_interval = instruction_budget(speed_factor)?;
        self.instructions_per_interval = instructions_per_interval;
        #[cfg(feature = "webgpu")]
        {
            self.speed_factor = speed_factor;
            if let Some(renderer) = self.extended_webgpu_renderer.as_mut() {
                renderer.set_speed_factor(speed_factor);
            }
        }
        Ok(())
    }

    /// Returns the remaining delay to the earliest scheduled erase completion.
    pub fn next_erase_completion(&self) -> Option<Duration> {
        self.erase_completions.front().copied()
    }

    /// Advances erase timeouts and Extended WebGPU phosphor time, including
    /// when execution is paused or stopped after an error.
    ///
    /// Hosts use this while displaying a runtime error; normal and paused
    /// `advance` calls already include this work and must not count it twice.
    pub fn advance_erase_callbacks(&mut self, machine: &mut Machine, elapsed: Duration) {
        self.sync_display(machine);
        self.sync_erase_requests(machine);
        self.elapse_erase_callbacks(elapsed);
        self.complete_due_erases(machine);
        #[cfg(feature = "webgpu")]
        self.advance_extended_phosphor(elapsed);
    }

    /// Advances machine work without dropping elapsed intervals.
    ///
    /// At a shared 10 ms deadline, CPU execution precedes the keyboard callback,
    /// matching the order in which the reference registers those intervals.
    /// The returned display-refresh count lets a presentation layer request a repaint
    /// after every due phosphor update rather than skipping updates after a slow frame.
    ///
    /// If CPU execution fails, due display refreshes at or before that callback
    /// are included in the error. The runtime stops later interval work, while
    /// erase timeouts and Extended WebGPU phosphor consume the remaining elapsed
    /// time. Reset before resuming; later advances return `Stopped` but still
    /// advance both.
    pub fn advance(
        &mut self,
        machine: &mut Machine,
        elapsed: Duration,
    ) -> Result<RuntimeEvents, HostRuntimeError> {
        if self.stopped {
            self.advance_erase_callbacks(machine, elapsed);
            return Err(HostRuntimeError::Stopped);
        }
        if self.paused {
            self.advance_erase_callbacks(machine, elapsed);
            return Ok(RuntimeEvents::default());
        }

        self.sync_display(machine);
        self.sync_erase_requests(machine);
        let mut remaining = elapsed;
        let mut events = RuntimeEvents::default();

        while !remaining.is_zero() {
            let mut step = remaining
                .min(EXECUTION_AND_KEYBOARD_INTERVAL - self.execution_elapsed)
                .min(DISPLAY_REFRESH_INTERVAL - self.display_elapsed);
            if let Some(delay) = self.next_erase_completion() {
                step = step.min(delay);
            }

            self.execution_elapsed += step;
            self.display_elapsed += step;
            self.elapse_erase_callbacks(step);
            #[cfg(feature = "webgpu")]
            self.advance_extended_phosphor(step);
            remaining -= step;

            let execution_due = self.execution_elapsed == EXECUTION_AND_KEYBOARD_INTERVAL;
            let display_due = self.display_elapsed == DISPLAY_REFRESH_INTERVAL;

            if execution_due {
                self.execution_elapsed = Duration::ZERO;
            }
            if display_due {
                self.display_elapsed = Duration::ZERO;
            }
            let execution_error = if execution_due {
                let error = machine.run_steps(self.instructions_per_interval).err();
                self.sync_display(machine);
                error
            } else {
                None
            };
            if execution_due {
                machine.keyboard_interrupt_tick();
                self.sync_erase_requests(machine);
            }

            if display_due {
                #[cfg(feature = "webgpu")]
                let revision_before = self.renderer.revision();
                self.renderer.refresh();
                #[cfg(feature = "webgpu")]
                if let Some(renderer) = self.webgpu_renderer.as_mut()
                    && let Err(error) = renderer.refresh_display()
                {
                    self.webgpu_renderer = None;
                    self.render_error = Some(format!(
                        "WebGPU phosphor refresh failed; using the CPU reference renderer: {error}"
                    ));
                    self.presentation_revision = self.presentation_revision.wrapping_add(1);
                }
                #[cfg(feature = "webgpu")]
                let extended_revision_before = self
                    .extended_webgpu_renderer
                    .as_ref()
                    .map(ExtendedWebGpuRenderer::revision);
                #[cfg(feature = "webgpu")]
                if let Some(renderer) = self.extended_webgpu_renderer.as_mut()
                    && let Err(error) = renderer.refresh_display()
                {
                    self.extended_webgpu_renderer = None;
                    self.render_error = Some(format!(
                        "Extended WebGPU phosphor refresh failed; using the CPU reference renderer: {error}"
                    ));
                    self.presentation_revision = self.presentation_revision.wrapping_add(1);
                }
                #[cfg(feature = "webgpu")]
                if self
                    .extended_webgpu_renderer
                    .as_ref()
                    .map(ExtendedWebGpuRenderer::revision)
                    != extended_revision_before
                {
                    self.presentation_revision = self.presentation_revision.wrapping_add(1);
                }
                #[cfg(feature = "webgpu")]
                if self.renderer.revision() != revision_before {
                    self.presentation_revision = self.presentation_revision.wrapping_add(1);
                }
                events.display_refreshes += 1;
            }

            self.complete_due_erases(machine);

            if let Some(source) = execution_error {
                self.stop();
                self.advance_erase_callbacks(machine, remaining);
                return Err(HostRuntimeError::Machine { source, events });
            }
        }
        Ok(events)
    }

    #[cfg(feature = "webgpu")]
    fn advance_extended_phosphor(&mut self, elapsed: Duration) {
        let revision_before = self
            .extended_webgpu_renderer
            .as_ref()
            .map(ExtendedWebGpuRenderer::revision);
        if let Some(renderer) = self.extended_webgpu_renderer.as_mut()
            && let Err(error) = renderer
                .advance_elapsed(elapsed)
                .and_then(|()| renderer.finish_display_batch())
                .and_then(|()| renderer.compose_frame())
        {
            self.extended_webgpu_renderer = None;
            self.render_error = Some(format!(
                "Extended WebGPU phosphor update failed; using the CPU reference renderer: {error}"
            ));
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        } else if self
            .extended_webgpu_renderer
            .as_ref()
            .is_some_and(|renderer| Some(renderer.revision()) != revision_before)
        {
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
    }

    fn stop(&mut self) {
        self.reset_elapsed();
        self.stopped = true;
        self.paused = false;
    }

    fn reset_elapsed(&mut self) {
        self.execution_elapsed = Duration::ZERO;
        self.display_elapsed = Duration::ZERO;
    }

    fn sync_erase_requests(&mut self, machine: &mut Machine) {
        self.erase_completions.extend(std::iter::repeat_n(
            DISPLAY_ERASE_DELAY,
            machine.take_display_erase_requests(),
        ));
    }

    fn elapse_erase_callbacks(&mut self, elapsed: Duration) {
        for delay in &mut self.erase_completions {
            *delay = delay.saturating_sub(elapsed);
        }
    }

    fn complete_due_erases(&mut self, machine: &mut Machine) {
        while self.erase_completions.front() == Some(&Duration::ZERO) {
            self.erase_completions.pop_front();
            machine.complete_display_erase();
        }
        self.sync_display(machine);
    }
}

fn instruction_budget(speed_factor: f64) -> Result<usize, SpeedFactorError> {
    if !speed_factor.is_finite() || speed_factor <= 0.0 {
        return Err(SpeedFactorError::NotFiniteAndPositive);
    }

    let speed_divisor = REFERENCE_SPEED_DIVISOR / speed_factor;
    let instructions = ((REFERENCE_MIPS * 1_000_000.0) / speed_divisor).ceil();
    if !instructions.is_finite() || instructions >= usize::MAX as f64 {
        return Err(SpeedFactorError::InstructionBudgetOverflow);
    }

    Ok(instructions as usize)
}
