//! Coordinates CPU execution with machine-bus signals for the Tektronix 4051.

use crate::cpu::{Cpu6800, CpuError, CpuRegisters, MemoryBus, StepOutcome};
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
    pub fn reset(&mut self) -> Result<(), CpuError<MachineBusError>> {
        self.irq_seen = false;
        self.nmi_seen = false;
        self.cpu.reset(&mut self.bus)
    }

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

    /// Execute no more than `instruction_budget` instructions.
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

    pub fn keyboard_interrupt_tick(&mut self) {
        self.bus.keyboard_interrupt_tick();
    }

    /// Marks the display ready at execution start or after an erase completes.
    /// The host owns the reference's 500 ms erase delay; stepping does not time it.
    pub fn display_ready(&mut self) {
        self.bus.display_ready();
    }

    /// Remains set until the host completes the erase with `display_ready`.
    pub fn display_erase_pending(&self) -> bool {
        self.bus.display_erase_pending()
    }

    pub fn bank_selection(&self) -> (u8, u8) {
        self.bus.bank_selection()
    }
}
