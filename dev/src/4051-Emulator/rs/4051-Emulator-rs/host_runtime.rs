//! Host-owned timing for the Tektronix 4051 machine.
//!
//! The web reference runs CPU execution and keyboard refresh callbacks every
//! 10 ms, display phosphor updates every 100 ms, and signals display readiness
//! 500 ms after an erase. This adapter applies those intervals from caller-
//! supplied elapsed time without adding clocks or delays to `Machine`.

use std::{error::Error, fmt, time::Duration};

use crate::{
    cpu::CpuError,
    machine::{Machine, MachineBusError},
};

const EXECUTION_AND_KEYBOARD_INTERVAL: Duration = Duration::from_millis(10);
const DISPLAY_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const DISPLAY_ERASE_DELAY: Duration = Duration::from_millis(500);
const REFERENCE_MIPS: f64 = 0.833;
const REFERENCE_SPEED_DIVISOR: f64 = 368.0;
const DEFAULT_INSTRUCTIONS_PER_INTERVAL: usize = 2_264;

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
/// display-erase completion calls `Machine::display_ready` after the reference delay.
pub struct HostRuntime {
    execution_elapsed: Duration,
    display_elapsed: Duration,
    erase_elapsed: Option<Duration>,
    instructions_per_interval: usize,
    stopped: bool,
}

impl HostRuntime {
    /// Starts host timing and applies the reference's initial display-ready signal.
    ///
    /// The caller remains responsible for resetting `machine` before starting it.
    pub fn start(machine: &mut Machine) -> Self {
        machine.display_ready();

        Self {
            execution_elapsed: Duration::ZERO,
            display_elapsed: Duration::ZERO,
            erase_elapsed: None,
            instructions_per_interval: DEFAULT_INSTRUCTIONS_PER_INTERVAL,
            stopped: false,
        }
    }

    /// Sets the reference speed factor used to calculate each 10 ms CPU budget.
    ///
    /// As in the web CPU, a fractional instruction count executes the next whole
    /// instruction because the reference loop compares an integer counter with
    /// the floating-point budget. Invalid values leave the current budget intact.
    pub fn set_speed_factor(&mut self, speed_factor: f64) -> Result<(), SpeedFactorError> {
        let instructions_per_interval = instruction_budget(speed_factor)?;
        self.instructions_per_interval = instructions_per_interval;
        Ok(())
    }

    /// Advances machine work without dropping elapsed intervals.
    ///
    /// At a shared 10 ms deadline, CPU execution precedes the keyboard callback,
    /// matching the order in which the reference registers those intervals.
    /// The returned display-refresh count lets a presentation layer request a repaint
    /// after every due phosphor update rather than skipping updates after a slow frame.
    ///
    /// If CPU execution fails, due display refreshes at or before that callback
    /// are included in the error. The runtime stops and discards later elapsed
    /// time from this call. Reset the machine and create a new runtime before
    /// resuming; later advances on this instance return `Stopped`.
    pub fn advance(
        &mut self,
        machine: &mut Machine,
        elapsed: Duration,
    ) -> Result<RuntimeEvents, HostRuntimeError> {
        if self.stopped {
            return Err(HostRuntimeError::Stopped);
        }

        self.sync_erase_state(machine);
        let mut remaining = elapsed;
        let mut events = RuntimeEvents::default();

        while !remaining.is_zero() {
            let mut step = remaining
                .min(EXECUTION_AND_KEYBOARD_INTERVAL - self.execution_elapsed)
                .min(DISPLAY_REFRESH_INTERVAL - self.display_elapsed);
            if let Some(erase_elapsed) = self.erase_elapsed {
                step = step.min(DISPLAY_ERASE_DELAY - erase_elapsed);
            }

            self.execution_elapsed += step;
            self.display_elapsed += step;
            if let Some(erase_elapsed) = self.erase_elapsed {
                self.erase_elapsed = Some(erase_elapsed + step);
            }
            remaining -= step;

            let execution_due = self.execution_elapsed == EXECUTION_AND_KEYBOARD_INTERVAL;
            let display_due = self.display_elapsed == DISPLAY_REFRESH_INTERVAL;
            let erase_due = self.erase_elapsed == Some(DISPLAY_ERASE_DELAY);

            if execution_due {
                self.execution_elapsed = Duration::ZERO;
            }
            if display_due {
                self.display_elapsed = Duration::ZERO;
            }
            if erase_due {
                self.erase_elapsed = None;
            }

            let execution_error = if execution_due {
                machine.run_steps(self.instructions_per_interval).err()
            } else {
                None
            };
            if execution_due {
                machine.keyboard_interrupt_tick();
                if execution_error.is_none() {
                    self.sync_erase_state(machine);
                }
            }

            if display_due {
                machine.refresh_display();
                events.display_refreshes += 1;
            }

            if erase_due {
                machine.display_ready();
                self.erase_elapsed = None;
            }

            if let Some(source) = execution_error {
                self.stop();
                return Err(HostRuntimeError::Machine { source, events });
            }
        }
        Ok(events)
    }

    fn stop(&mut self) {
        self.execution_elapsed = Duration::ZERO;
        self.display_elapsed = Duration::ZERO;
        self.erase_elapsed = None;
        self.stopped = true;
    }

    fn sync_erase_state(&mut self, machine: &Machine) {
        if machine.display_erase_pending() {
            self.erase_elapsed.get_or_insert(Duration::ZERO);
        } else {
            self.erase_elapsed = None;
        }
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
