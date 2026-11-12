//! Implements the 4051 machine's RAM, ROM, I/O, and PIA-backed interrupt signals.
//!
//! I/O is decoded before ROM, and the bank register affects only its switchable
//! window. PIA accesses also drive the display and keyboard state; unsupported
//! ROM-window reads return an error instead of falling back to the base image.

use std::{error::Error, fmt};

#[cfg(feature = "wasm")]
use crate::flash_drive::archive::OwnedArchiveFile;

#[cfg(test)]
use super::BasicKeyboardLatch;
use super::MachineIndicators;
use super::cpu::MemoryBus;
use super::display::{DisplayControl, DisplayOperation, ScreenCommand};
use super::gpib::{
    FlashDriveExportError, FlashDriveFile, FlashDriveFileInfo, FlashDriveImportError,
    GpibController, GpibError, RtcClock, TapeFileNumberError,
};
use super::pia::{PiaControlLine, PiaId, PiaPort, PiaRegisters};
use super::rom::{
    BASE_ROM, LEFT_BACKPACK_ROM, LEFT_ROM_EXPANDER_BANK, OVERFLOW_ROM, RIGHT_BACKPACK_ROM,
    RIGHT_ROM_EXPANDER_BANK, ROM_EXPANDER_SLOT_SIZE,
};

const RAM_SIZE: usize = 32 * 1024;
const ROM_BASE: u16 = 0x8000;
const IO_START: u16 = 0x8780;
const IO_END: u16 = 0x87ff;
const BANK_SELECT_REGISTER: u16 = 0x87c0;
#[cfg(test)]
const BASE_ROM_KEY_DOWN: u16 = 0x0597;
#[cfg(test)]
const BASE_ROM_KEY_CODE: u16 = 0x059c;
const BANK_SWITCH_WINDOW_START: u16 = 0x8800;
const BANK_SWITCH_WINDOW_END: u16 = 0xa800;
const OVERFLOW_ROM_END: u16 = 0x9800;
const PIA_BASES: [u16; 5] = [0x878c, 0x8794, 0x8798, 0x87a8, 0x87b0];
const DISPLAY_READY_MASK: u8 = 0x04;
const DISPLAY_ERASE_REQUEST_MASK: u8 = 0x10;
const DISPLAY_COPY_REQUEST_MASK: u8 = 0x20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineBusError {
    UnsupportedRomBank { selector: u8, address: u16 },
    GpibDeviceNotImplemented { address: u8 },
    GpibCommandNotImplemented { address: u8, command: u8 },
    RtcClockUnavailable { detail: String },
    UnsupportedTapeSelection { bytes: Vec<u8> },
    InvalidTapeHeader { bytes: Vec<u8> },
    TapeFileNotSelected,
    TapePositionOverflow,
}

impl fmt::Display for MachineBusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedRomBank { selector, address } => write!(
                formatter,
                "ROM bank selector {selector} is not implemented for address {address:#06x}"
            ),
            Self::GpibDeviceNotImplemented { address } => {
                write!(
                    formatter,
                    "GPIB device at address {address} is not implemented"
                )
            }
            Self::GpibCommandNotImplemented { address, command } => {
                write!(
                    formatter,
                    "GPIB device at address {address} does not implement command {command:#04x}"
                )
            }
            Self::RtcClockUnavailable { detail } => {
                write!(formatter, "GPIB RTC could not read local time: {detail}")
            }
            Self::UnsupportedTapeSelection { bytes } => {
                write!(
                    formatter,
                    "GPIB tape selection {bytes:?} requires an unsupported file dialog"
                )
            }
            Self::TapeFileNotSelected => {
                write!(formatter, "GPIB tape write requires a file number")
            }
            Self::InvalidTapeHeader { bytes } => {
                write!(formatter, "GPIB tape LIST header {bytes:?} is invalid")
            }
            Self::TapePositionOverflow => write!(formatter, "GPIB tape record position overflow"),
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
    gpib: GpibController,
    bank_select: u8,
    bank_address: u8,
    keyboard_shift_0: bool,
    keyboard_ctrl_0: bool,
    keyboard_tty_0: bool,
    display_erase_pending: bool,
    display_erase_requests: Vec<()>,
    display_operations: Vec<DisplayOperation>,
    #[cfg(test)]
    cpu_instruction_active: bool,
    #[cfg(test)]
    base_rom_keyboard_latch_seen: bool,
}

impl Default for MachineBus {
    fn default() -> Self {
        let mut pias = [PiaRegisters::default(); PIA_BASES.len()];
        pias[PiaId::Keyboard as usize].set_input(PiaPort::B, 0x0b);
        Self {
            ram: Box::new([0; RAM_SIZE]),
            pias,
            gpib: GpibController::default(),
            bank_select: 0,
            bank_address: 0,
            keyboard_shift_0: true,
            keyboard_ctrl_0: true,
            keyboard_tty_0: true,
            display_erase_pending: false,
            display_erase_requests: Vec::new(),
            display_operations: Vec::new(),
            #[cfg(test)]
            cpu_instruction_active: false,
            #[cfg(test)]
            base_rom_keyboard_latch_seen: false,
        }
    }
}

impl MachineBus {
    #[cfg(test)]
    pub(crate) fn set_cpu_instruction_active(&mut self, active: bool) {
        self.cpu_instruction_active = active;
    }

    #[cfg(test)]
    pub(crate) fn basic_keyboard_latch(&self) -> BasicKeyboardLatch {
        if !self.base_rom_keyboard_latch_seen {
            return BasicKeyboardLatch::Unavailable;
        }
        if self.ram[usize::from(BASE_ROM_KEY_DOWN)] == 0 {
            BasicKeyboardLatch::Released
        } else {
            BasicKeyboardLatch::Pressed(self.ram[usize::from(BASE_ROM_KEY_CODE)])
        }
    }

    fn read_io(&mut self, address: u16) -> u8 {
        for (index, base) in PIA_BASES.iter().copied().enumerate() {
            if (base..base + 4).contains(&address) {
                let port = address - base;
                if index == PiaId::Gpib as usize {
                    if port == 0 && self.pias[index].is_data_direction_register_selected(PiaPort::A)
                    {
                        self.pias[index].set_input(PiaPort::A, self.gpib.data_input());
                    } else if port == 2
                        && self.pias[index].is_data_direction_register_selected(PiaPort::B)
                    {
                        self.pias[index].set_input(PiaPort::B, self.gpib.read_control_input());
                    }
                } else if index == PiaId::Keyboard as usize
                    && port == 2
                    && self.pias[index].is_data_direction_register_selected(PiaPort::B)
                {
                    self.pias[index].set_input_bit(PiaPort::B, 0x10, self.gpib.eoi_input());
                }
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
                    self.display_operations
                        .push(DisplayOperation::ScreenCommand { command, control });
                }
                return self.pias[index].read(port);
            }
        }
        0
    }

    fn write_io(&mut self, address: u16, value: u8) -> Result<(), MachineBusError> {
        if address == BANK_SELECT_REGISTER {
            self.bank_address = value & 0x07;
            self.bank_select = (value >> 3) & 0x07;
            return Ok(());
        }
        for (index, base) in PIA_BASES.iter().copied().enumerate() {
            if (base..base + 4).contains(&address) {
                let port = address - base;
                let erase_started;
                let buffer_clock;
                let copy_requested;
                {
                    let pia = &mut self.pias[index];
                    copy_requested = index == PiaId::DisplayX as usize
                        && port == 0
                        && pia.is_data_direction_register_selected(PiaPort::A)
                        && pia.output_bit_is_high(PiaPort::A, DISPLAY_COPY_REQUEST_MASK)
                        && value & DISPLAY_COPY_REQUEST_MASK == 0;
                    erase_started = index == PiaId::DisplayX as usize
                        && port == 0
                        && pia.is_data_direction_register_selected(PiaPort::A)
                        && pia.output_bit_is_high(PiaPort::A, DISPLAY_ERASE_REQUEST_MASK)
                        && value & DISPLAY_ERASE_REQUEST_MASK == 0;
                    pia.write(port, value);
                    // BUFCLK follows the DisplayX port-B output-latch write.
                    buffer_clock = index == PiaId::DisplayX as usize
                        && port == 2
                        && pia.is_data_direction_register_selected(PiaPort::B);
                }
                if index == PiaId::Gpib as usize {
                    let result = match port {
                        0 if self.pias[index].is_data_direction_register_selected(PiaPort::A) => {
                            self.gpib.set_data_output(value);
                            Ok(())
                        }
                        1 => self.gpib.write_control_a(value),
                        2 if self.pias[index].is_data_direction_register_selected(PiaPort::B) => {
                            self.gpib.write_port_b(value)
                        }
                        3 => self.gpib.write_control_b(value),
                        _ => Ok(()),
                    };
                    result.map_err(|error| match error {
                        GpibError::CommandNotImplemented { address, command } => {
                            MachineBusError::GpibCommandNotImplemented { address, command }
                        }
                        GpibError::RtcClock(error) => MachineBusError::RtcClockUnavailable {
                            detail: error.to_string(),
                        },
                        GpibError::UnsupportedTapeSelection(bytes) => {
                            MachineBusError::UnsupportedTapeSelection { bytes }
                        }
                        GpibError::InvalidTapeHeader(bytes) => {
                            MachineBusError::InvalidTapeHeader { bytes }
                        }
                        GpibError::TapeFileNotSelected => MachineBusError::TapeFileNotSelected,
                        GpibError::TapePositionOverflow => MachineBusError::TapePositionOverflow,
                    })?;
                } else if index == PiaId::Keyboard as usize
                    && port == 2
                    && self.pias[index].is_data_direction_register_selected(PiaPort::B)
                {
                    self.gpib.set_keyboard_eoi(value);
                }
                if erase_started {
                    self.erase_display();
                }
                if copy_requested {
                    self.display_operations.push(DisplayOperation::ScreenCopy);
                }
                if buffer_clock {
                    let control = self.display_control();
                    self.display_operations
                        .push(DisplayOperation::ScreenCommand {
                            command: ScreenCommand::BufferClock,
                            control,
                        });
                }
                return Ok(());
            }
        }
        Ok(())
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
        self.pias[PiaId::DisplayX as usize].has_interrupt_flags()
            || self.pias[PiaId::Keyboard as usize].has_interrupt_flags()
            || self.pias[PiaId::Gpib as usize].has_interrupt_flags()
    }

    pub(crate) fn nmi_pending(&self) -> bool {
        self.pias[PiaId::DisplayY as usize].has_interrupt_flags()
            || self.pias[PiaId::Tape as usize].has_interrupt_flags()
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

    pub(crate) fn load_program(&mut self, bytes: &[u8]) {
        self.gpib.load_program(bytes);
    }

    pub(crate) fn set_rtc_clock(&mut self, clock: RtcClock) {
        self.gpib.set_rtc_clock(clock);
    }

    pub(crate) fn import_flash_files(
        &mut self,
        files: &[FlashDriveFile],
    ) -> Vec<Result<(), FlashDriveImportError>> {
        self.gpib.import_flash_files(files)
    }

    pub(crate) fn flash_drive_directory(&self) -> Vec<FlashDriveFileInfo> {
        self.gpib.flash_drive_directory()
    }

    pub(crate) fn write_flash_storage_archive<W: std::io::Write + std::io::Seek>(
        &self,
        destination: W,
    ) -> Result<W, String> {
        self.gpib.write_flash_storage_archive(destination)
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn flash_drive_archive_file(&self, number: u8) -> Option<OwnedArchiveFile> {
        self.gpib.flash_drive_archive_file(number)
    }

    pub(crate) fn import_flash_storage_archive<R: std::io::Read + std::io::Seek>(
        &mut self,
        source: R,
    ) -> Result<(), String> {
        self.gpib.import_flash_storage_archive(source)
    }

    pub(crate) fn clear_flash_drive(&mut self) {
        self.gpib.clear_flash_drive();
    }

    pub(crate) fn export_flash_file(
        &self,
        number: u8,
    ) -> Result<FlashDriveFile, FlashDriveExportError> {
        self.gpib.export_flash_file(number)
    }

    pub(crate) fn export_flash_file_name(
        &self,
        number: u8,
    ) -> Result<String, FlashDriveExportError> {
        self.gpib.export_flash_file_name(number)
    }

    pub(crate) fn store_tape_program(
        &mut self,
        number: u8,
        bytes: &[u8],
    ) -> Result<(), TapeFileNumberError> {
        self.gpib.store_tape_program(number, bytes)
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

    pub(crate) fn indicators(&self) -> MachineIndicators {
        let output = self.pias[PiaId::Keyboard as usize].output(PiaPort::B);
        MachineIndicators {
            busy: output & 0x80 != 0,
            io: output & 0x40 != 0,
            break_active: output & 0x20 != 0,
        }
    }

    pub(crate) fn keyboard_interrupt_tick(&mut self) {
        self.pias[PiaId::Keyboard as usize].raise_ca1_interrupt();
    }

    pub(crate) fn display_ready(&mut self) {
        if self.display_erase_pending {
            self.display_operations
                .push(DisplayOperation::CompleteErase);
        }
        self.pias[PiaId::DisplayX as usize].set_input_bit(PiaPort::A, DISPLAY_READY_MASK, true);
        self.display_erase_pending = false;
    }

    pub(crate) fn erase_display(&mut self) {
        self.pias[PiaId::DisplayX as usize].set_input_bit(PiaPort::A, DISPLAY_READY_MASK, false);
        self.display_erase_pending = true;
        // Each web ERASE call schedules its own timeout, even while another erase is pending.
        self.display_erase_requests.push(());
        self.display_operations.push(DisplayOperation::BeginErase);
    }

    pub(crate) fn take_display_erase_requests(&mut self) -> usize {
        let requests = self.display_erase_requests.len();
        self.display_erase_requests.clear();
        requests
    }

    pub(crate) fn complete_display_erase(&mut self) {
        // A later timeout still blanks the screen after an earlier one signaled readiness.
        self.display_operations
            .push(DisplayOperation::CompleteErase);
        self.pias[PiaId::DisplayX as usize].set_input_bit(PiaPort::A, DISPLAY_READY_MASK, true);
        self.display_erase_pending = false;
    }

    pub(crate) fn take_display_operations(&mut self) -> Vec<DisplayOperation> {
        std::mem::take(&mut self.display_operations)
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
        if address < ROM_BASE {
            return Ok(self.ram[usize::from(address)]);
        }
        if (IO_START..=IO_END).contains(&address) {
            return Ok(self.read_io(address));
        }
        // A nonzero selector changes only the bank window, not the rest of ROM.
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
        Ok(BASE_ROM[offset])
    }

    fn write_byte(&mut self, address: u16, value: u8) -> Result<(), Self::Error> {
        if (IO_START..=IO_END).contains(&address) {
            self.write_io(address, value)?;
        } else if address < ROM_BASE {
            #[cfg(test)]
            if self.cpu_instruction_active
                && matches!(address, BASE_ROM_KEY_DOWN | BASE_ROM_KEY_CODE)
            {
                self.base_rom_keyboard_latch_seen = true;
            }
            self.ram[usize::from(address)] = value;
        }
        Ok(())
    }
}
