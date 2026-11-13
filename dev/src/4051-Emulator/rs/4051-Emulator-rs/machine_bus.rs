//! Implements the 4051 machine's RAM, ROM, I/O, and PIA-backed interrupt signals.
//!
//! I/O is decoded before ROM, and the bank register affects only its switchable
//! window. PIA accesses also drive the display and keyboard state; unsupported
//! ROM-window reads return an error instead of falling back to the base image.

use std::{error::Error, fmt};

use crate::cpu::MemoryBus;
use crate::display::{DisplayControl, DisplayGranularity, DisplayModel, ScreenCommand};
use crate::pia::{PiaControlLine, PiaId, PiaPort, PiaRegisters};
use crate::rom::{
    BASE_ROM, LEFT_BACKPACK_ROM, LEFT_ROM_EXPANDER_BANK, OVERFLOW_ROM, RIGHT_BACKPACK_ROM,
    RIGHT_ROM_EXPANDER_BANK, ROM_EXPANDER_SLOT_SIZE,
};

const RAM_SIZE: usize = 32 * 1024;
const ROM_BASE: u16 = 0x8000;
const IO_START: u16 = 0x8780;
const IO_END: u16 = 0x87ff;
const BANK_SELECT_REGISTER: u16 = 0x87c0;
const BANK_SWITCH_WINDOW_START: u16 = 0x8800;
const BANK_SWITCH_WINDOW_END: u16 = 0xa800;
const OVERFLOW_ROM_END: u16 = 0x9800;
const PIA_BASES: [u16; 5] = [0x878c, 0x8794, 0x8798, 0x87a8, 0x87b0];
const DISPLAY_READY_MASK: u8 = 0x04;
const DISPLAY_ERASE_REQUEST_MASK: u8 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineBusError {
    UnsupportedRomBank { selector: u8, address: u16 },
}

impl fmt::Display for MachineBusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedRomBank { selector, address } => write!(
                formatter,
                "ROM bank selector {selector} is not implemented for address {address:#06x}"
            ),
        }
    }
}

impl Error for MachineBusError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardModifier {
    Shift,
    Control,
}

pub(crate) struct MachineBus {
    ram: Box<[u8; RAM_SIZE]>,
    pias: [PiaRegisters; PIA_BASES.len()],
    bank_select: u8,
    bank_address: u8,
    keyboard_shift_0: bool,
    keyboard_ctrl_0: bool,
    keyboard_tty_0: bool,
    display_erase_pending: bool,
    display: DisplayModel,
}

impl Default for MachineBus {
    fn default() -> Self {
        let mut pias = [PiaRegisters::default(); PIA_BASES.len()];
        pias[PiaId::Keyboard as usize].set_input(PiaPort::B, 0x0b);
        Self {
            ram: Box::new([0; RAM_SIZE]),
            pias,
            bank_select: 0,
            bank_address: 0,
            keyboard_shift_0: true,
            keyboard_ctrl_0: true,
            keyboard_tty_0: true,
            display_erase_pending: false,
            display: DisplayModel::default(),
        }
    }
}

impl MachineBus {
    fn read_io(&mut self, address: u16) -> u8 {
        for (index, base) in PIA_BASES.iter().copied().enumerate() {
            if (base..base + 4).contains(&address) {
                let port = address - base;
                // The web board invokes SOT/ADOT on data-register reads, before returning PIA data.
                let command = match (index, port) {
                    (pia, 0)
                        if pia == PiaId::DisplayY as usize
                            && self.pias[pia].is_data_direction_register_selected(PiaPort::A) =>
                    {
                        Some(ScreenCommand::StartOfText)
                    }
                    (pia, 0)
                        if pia == PiaId::DisplayX as usize
                            && self.pias[pia].is_data_direction_register_selected(PiaPort::A) =>
                    {
                        Some(ScreenCommand::AlphaDot)
                    }
                    _ => None,
                };
                if let Some(command) = command {
                    let control = self.display_control();
                    self.display.apply_screen_command(command, control);
                }
                return self.pias[index].read(port);
            }
        }
        0
    }

    fn write_io(&mut self, address: u16, value: u8) {
        if address == BANK_SELECT_REGISTER {
            self.bank_address = value & 0x07;
            self.bank_select = (value >> 3) & 0x07;
            return;
        }
        for (index, base) in PIA_BASES.iter().copied().enumerate() {
            if (base..base + 4).contains(&address) {
                let port = address - base;
                let erase_started;
                let buffer_clock;
                {
                    let pia = &mut self.pias[index];
                    erase_started = index == PiaId::DisplayX as usize
                        && port == 0
                        && pia.is_data_direction_register_selected(PiaPort::A)
                        && pia.output_bit_is_high(PiaPort::A, DISPLAY_ERASE_REQUEST_MASK)
                        && value & DISPLAY_ERASE_REQUEST_MASK == 0;
                    if erase_started {
                        pia.set_input_bit(PiaPort::A, DISPLAY_READY_MASK, false);
                    }
                    pia.write(port, value);
                    // BUFCLK follows the DisplayX port-B output-latch write.
                    buffer_clock = index == PiaId::DisplayX as usize
                        && port == 2
                        && pia.is_data_direction_register_selected(PiaPort::B);
                }
                if erase_started {
                    self.display_erase_pending = true;
                    self.display.begin_erase();
                }
                if buffer_clock {
                    let control = self.display_control();
                    self.display
                        .apply_screen_command(ScreenCommand::BufferClock, control);
                }
                return;
            }
        }
    }

    fn display_control(&self) -> DisplayControl {
        let display_y = &self.pias[PiaId::DisplayY as usize];
        let display_x = &self.pias[PiaId::DisplayX as usize];
        // The display controller samples the PIA output latches, not the merged pin reads.
        let y_output_a = display_y.output(PiaPort::A);
        let x_output_a = display_x.output(PiaPort::A);
        DisplayControl {
            x_character: (y_output_a >> 5) & 0x07,
            y_character: (y_output_a >> 2) & 0x07,
            vector_0: x_output_a & 0x40 != 0,
            ven_1: x_output_a & 0x80 != 0,
            x: (u16::from(x_output_a & 0x03) << 8) | u16::from(display_x.output(PiaPort::B)),
            y: (u16::from(y_output_a & 0x03) << 8) | u16::from(display_y.output(PiaPort::B)),
        }
    }

    // Board interrupt routing uses the latched PIA flags directly.
    pub(crate) fn irq_pending(&self) -> bool {
        [PiaId::DisplayX, PiaId::Keyboard, PiaId::Gpib]
            .into_iter()
            .any(|pia| self.pias[pia as usize].has_interrupt_flags())
    }

    pub(crate) fn nmi_pending(&self) -> bool {
        [PiaId::DisplayY, PiaId::Tape]
            .into_iter()
            .any(|pia| self.pias[pia as usize].has_interrupt_flags())
    }

    pub(crate) fn set_pia_input(&mut self, pia: PiaId, port: PiaPort, value: u8) {
        self.pias[pia as usize].set_input(port, value);
    }

    pub(crate) fn signal_pia_control_line(
        &mut self,
        pia: PiaId,
        line: PiaControlLine,
        level: bool,
    ) {
        self.pias[pia as usize].signal_control_line(line, level);
    }

    // Key data and active-low modifier levels occupy separate keyboard ports.
    pub(crate) fn keyboard_press(&mut self, key: u8) {
        self.pias[PiaId::Keyboard as usize].set_input(
            PiaPort::A,
            (u8::from(self.keyboard_tty_0) << 7) | (key & 0x7f),
        );
    }

    pub(crate) fn keyboard_release(&mut self) {
        self.pias[PiaId::Keyboard as usize]
            .set_input(PiaPort::A, 0x80 | (u8::from(self.keyboard_tty_0) << 7));
    }

    pub(crate) fn set_keyboard_modifier(&mut self, modifier: KeyboardModifier, pressed: bool) {
        let (level, mask) = match modifier {
            KeyboardModifier::Shift => (&mut self.keyboard_shift_0, 0x01),
            KeyboardModifier::Control => (&mut self.keyboard_ctrl_0, 0x02),
        };
        *level = !pressed;
        self.pias[PiaId::Keyboard as usize].set_input_bit(PiaPort::B, mask, *level);
    }

    pub(crate) fn toggle_caps_lock(&mut self) {
        self.keyboard_tty_0 = !self.keyboard_tty_0;
    }

    pub(crate) fn keyboard_interrupt_tick(&mut self) {
        self.pias[PiaId::Keyboard as usize].raise_ca1_interrupt();
    }

    pub(crate) fn display_ready(&mut self) {
        if self.display_erase_pending {
            self.display.complete_erase();
        }
        self.pias[PiaId::DisplayX as usize].set_input_bit(PiaPort::A, DISPLAY_READY_MASK, true);
        self.display_erase_pending = false;
    }

    pub(crate) fn display(&self) -> &DisplayModel {
        &self.display
    }

    pub(crate) fn set_display_granularity(&mut self, granularity: DisplayGranularity) {
        self.display.set_granularity(granularity);
    }

    pub(crate) fn refresh_display(&mut self) {
        self.display.refresh();
    }

    pub(crate) fn display_erase_pending(&self) -> bool {
        self.display_erase_pending
    }

    pub(crate) fn bank_selection(&self) -> (u8, u8) {
        (self.bank_select, self.bank_address)
    }
}

impl MemoryBus for MachineBus {
    type Error = MachineBusError;

    fn read_byte(&mut self, address: u16) -> Result<u8, Self::Error> {
        if (IO_START..=IO_END).contains(&address) {
            return Ok(self.read_io(address));
        }
        // A nonzero selector changes only the bank window, not the rest of ROM.
        if address >= ROM_BASE {
            if self.bank_select != 0
                && (BANK_SWITCH_WINDOW_START..BANK_SWITCH_WINDOW_END).contains(&address)
            {
                if self.bank_select == 1 && address < OVERFLOW_ROM_END {
                    return Ok(OVERFLOW_ROM[usize::from(address - BANK_SWITCH_WINDOW_START)]);
                }
                if self.bank_select == 4 {
                    // The reference array contains 2 KiB; keep its undefined tail explicit.
                    let offset = usize::from(address - BANK_SWITCH_WINDOW_START);
                    return LEFT_BACKPACK_ROM.get(offset).copied().ok_or(
                        MachineBusError::UnsupportedRomBank {
                            selector: self.bank_select,
                            address,
                        },
                    );
                }
                if self.bank_select == 5 {
                    let offset = usize::from(address - BANK_SWITCH_WINDOW_START);
                    return RIGHT_BACKPACK_ROM.get(offset).copied().ok_or(
                        MachineBusError::UnsupportedRomBank {
                            selector: self.bank_select,
                            address,
                        },
                    );
                }
                if self.bank_select == 6 {
                    // The register's address field selects one 8 KiB slot in the left bank.
                    let slot_offset = usize::from(self.bank_address) * ROM_EXPANDER_SLOT_SIZE;
                    let address_offset = usize::from(address - BANK_SWITCH_WINDOW_START);
                    return LEFT_ROM_EXPANDER_BANK
                        .get(slot_offset + address_offset)
                        .copied()
                        .ok_or(MachineBusError::UnsupportedRomBank {
                            selector: self.bank_select,
                            address,
                        });
                }
                if self.bank_select == 7 {
                    // The register's address field selects one 8 KiB slot in the right bank.
                    let slot_offset = usize::from(self.bank_address) * ROM_EXPANDER_SLOT_SIZE;
                    let address_offset = usize::from(address - BANK_SWITCH_WINDOW_START);
                    return RIGHT_ROM_EXPANDER_BANK
                        .get(slot_offset + address_offset)
                        .copied()
                        .ok_or(MachineBusError::UnsupportedRomBank {
                            selector: self.bank_select,
                            address,
                        });
                }
                // The reference ignores these banks; zero preserves its word-probe result.
                if matches!(self.bank_select, 2 | 3) {
                    return Ok(0);
                }
                return Err(MachineBusError::UnsupportedRomBank {
                    selector: self.bank_select,
                    address,
                });
            }
            let offset = usize::from(address - ROM_BASE);
            return Ok(BASE_ROM[offset]);
        }
        Ok(self.ram[usize::from(address)])
    }

    fn write_byte(&mut self, address: u16, value: u8) -> Result<(), Self::Error> {
        if (IO_START..=IO_END).contains(&address) {
            self.write_io(address, value);
        } else if address < ROM_BASE {
            self.ram[usize::from(address)] = value;
        }
        Ok(())
    }
}
