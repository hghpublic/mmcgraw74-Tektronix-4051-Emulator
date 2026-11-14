//! Coordinates CPU execution with machine-bus signals for the Tektronix 4051.
//!
//! Interrupt sources are sampled before each instruction. NMI takes priority;
//! held NMI is serviced once until cleared; a masked IRQ is reconsidered when
//! the CPU's interrupt mask is removed.

use crate::cpu::{Cpu6800, CpuError, CpuRegisters, MemoryBus, StepOutcome};
use crate::display::{DisplayGranularity, DisplayModel};
use crate::machine_bus::MachineBus;

pub use crate::machine_bus::{KeyboardModifier, MachineBusError};
pub use crate::pia::{PiaControlLine, PiaId, PiaPort};

/// A deterministic, UI-independent Tektronix 4051 machine core.
#[derive(Default)]
pub struct Machine {
    cpu: Cpu6800,
    bus: MachineBus,
    irq_seen: bool,
    nmi_seen: bool,
}

impl Machine {
    /// Resets the CPU and interrupt tracking; RAM and peripheral state are retained.
    pub fn reset(&mut self) -> Result<(), CpuError<MachineBusError>> {
        self.irq_seen = false;
        self.nmi_seen = false;
        self.cpu.reset(&mut self.bus)
    }

    /// Services an eligible interrupt, then executes one instruction.
    ///
    /// Bus and CPU errors are returned without rolling back earlier effects.
    pub fn step(&mut self) -> Result<StepOutcome, CpuError<MachineBusError>> {
        if self.bus.nmi_pending() {
            if !self.nmi_seen {
                self.cpu.service_nmi(&mut self.bus)?;
            }
            self.nmi_seen = true;
        } else {
            self.nmi_seen = false;
            if self.bus.irq_pending() {
                if self.cpu.registers.ccr & crate::cpu::INTERRUPT_MASK == 0 {
                    if !self.irq_seen {
                        self.cpu.service_irq(&mut self.bus)?;
                    }
                    self.irq_seen = true;
                } else {
                    self.irq_seen = false;
                }
            } else {
                self.irq_seen = false;
            }
        }
        self.cpu.step(&mut self.bus)
    }

    /// Executes at most `instruction_budget` instructions and returns the count.
    ///
    /// Stops at the first error; instructions completed earlier remain effective.
    pub fn run_steps(
        &mut self,
        instruction_budget: usize,
    ) -> Result<usize, CpuError<MachineBusError>> {
        let mut executed = 0;
        while executed < instruction_budget {
            self.step()?;
            executed += 1;
        }
        Ok(executed)
    }

    pub fn registers(&self) -> CpuRegisters {
        self.cpu.registers
    }

    pub fn cpu_mut(&mut self) -> &mut Cpu6800 {
        &mut self.cpu
    }

    /// Reads the mapped byte; I/O reads can have peripheral side effects.
    pub fn read_byte(&mut self, address: u16) -> Result<u8, MachineBusError> {
        self.bus.read_byte(address)
    }

    pub fn write_byte(&mut self, address: u16, value: u8) -> Result<(), MachineBusError> {
        self.bus.write_byte(address, value)
    }

    pub fn set_pia_input(&mut self, pia: PiaId, port: PiaPort, value: u8) {
        self.bus.set_pia_input(pia, port, value);
    }

    pub fn signal_pia_control_line(&mut self, pia: PiaId, line: PiaControlLine, level: bool) {
        self.bus.signal_pia_control_line(pia, line, level);
    }

    /// Places a seven-bit Tek key code on the keyboard input port.
    pub fn keyboard_press(&mut self, key: u8) {
        self.bus.keyboard_press(key);
    }

    pub fn keyboard_release(&mut self) {
        self.bus.keyboard_release();
    }

    pub fn set_keyboard_modifier(&mut self, modifier: KeyboardModifier, pressed: bool) {
        self.bus.set_keyboard_modifier(modifier, pressed);
    }

    pub fn toggle_caps_lock(&mut self) {
        self.bus.toggle_caps_lock();
    }

    /// Latches the keyboard PIA interrupt flag for a host keyboard callback.
    pub fn keyboard_interrupt_tick(&mut self) {
        self.bus.keyboard_interrupt_tick();
    }

    /// Marks the display ready at execution start or after an erase completes.
    /// The host owns the reference's 500 ms erase delay; stepping does not time it.
    pub fn display_ready(&mut self) {
        self.bus.display_ready();
    }

    /// Returns the machine's read-only display framebuffer and pixel settings.
    pub fn display(&self) -> &DisplayModel {
        self.bus.display()
    }

    /// Takes ordered framebuffer snapshots requested by the display-copy signal.
    ///
    /// Each snapshot contains `DisplayModel::WIDTH * DisplayModel::HEIGHT` green
    /// intensities in row-major order. The second return value counts requests
    /// dropped after the bounded queue filled, saturating at `usize::MAX`.
    pub fn take_screen_copies(&mut self) -> (Vec<Vec<u8>>, usize) {
        self.bus.take_screen_copies()
    }

    /// Selects the fine, coarse, or mixed display dot sizing mode.
    pub fn set_display_granularity(&mut self, granularity: DisplayGranularity) {
        self.bus.set_display_granularity(granularity);
    }

    pub(crate) fn refresh_display(&mut self) {
        self.bus.refresh_display();
    }

    /// Remains set until the host completes the erase with `display_ready`.
    pub fn display_erase_pending(&self) -> bool {
        self.bus.display_erase_pending()
    }

    /// Returns the current ROM selector and expander address field.
    pub fn bank_selection(&self) -> (u8, u8) {
        self.bus.bank_selection()
    }
}
