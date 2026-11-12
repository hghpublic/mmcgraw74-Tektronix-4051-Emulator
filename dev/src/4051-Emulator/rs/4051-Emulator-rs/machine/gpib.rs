//! GPIB transfer buffers, board handshake, Loader, and real-time clock.
//!
//! The board accepts control and data writes from the GPIB PIA, dispatches
//! addressed commands, and supplies data, DAV, NDAC, and EOI for PIA reads.
//! The Loader at address 1, local-time RTC at address 2, and part of the
//! default tape device at address 5 are implemented. RTC dispatch obtains a
//! calendar value from its configured callback; host sampling is outside GPIB.

use jiff::civil::DateTime;

pub use crate::flash_drive::archive::{
    MAX_FLASH_ARCHIVE_ENTRIES, MAX_FLASH_ARCHIVE_ENTRY_BYTES, MAX_FLASH_ARCHIVE_FILE_BYTES,
    MAX_FLASH_ARCHIVE_TOTAL_BYTES,
};

mod tape;
use crate::flash_drive::FileType;
pub use crate::flash_drive::FlashDriveExportError;
#[cfg(feature = "wasm")]
use crate::flash_drive::archive::OwnedArchiveFile;
pub use tape::{FlashDriveFile, FlashDriveFileInfo, FlashDriveImportError, TapeFileNumberError};
use tape::{Tape, TapeHeaderError, TapePrintError};

/// Supplies a local calendar value at each RTC read dispatch.
///
/// The callback's failures propagate as `MachineBusError::RtcClockUnavailable`.
/// A function pointer keeps machines movable to native workers without captured state.
pub type RtcClock = fn() -> Result<DateTime, jiff::Error>;

#[cfg(test)]
#[path = "gpib/tests.rs"]
mod tests;

/// Shared byte buffers used by GPIB device transactions.
///
/// Replacing input preserves the current read position, as in the reference.
/// Clear the input explicitly to discard a partially read transaction.
#[derive(Debug, Default)]
pub struct GpibBuffers {
    input: Vec<u8>,
    input_position: usize,
    output: Vec<u8>,
    byte_to_send: u8,
}

impl GpibBuffers {
    /// Returns the next input byte and whether EOI accompanies it.
    ///
    /// The last byte clears the input. Empty or exhausted input returns
    /// `0xff` with EOI set, matching the reference's poll behavior.
    pub fn poll(&mut self) -> (u8, bool) {
        match self.input.get(self.input_position).copied() {
            Some(byte) if self.input_position == self.input.len() - 1 => {
                self.clear_input();
                (byte, true)
            }
            Some(byte) => {
                self.input_position += 1;
                (byte, false)
            }
            None => (0xff, true),
        }
    }

    /// Replaces the input bytes without changing the current read position.
    pub fn replace_input(&mut self, bytes: Vec<u8>) {
        self.input = bytes;
    }

    /// Discards the input and resets its read position.
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.input_position = 0;
    }

    /// Appends a byte to the output accumulated for a device.
    pub fn append_output(&mut self, byte: u8) {
        self.output.push(byte);
    }

    /// Returns and clears the accumulated output bytes.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Discards the accumulated output bytes.
    pub fn clear_output(&mut self) {
        self.output.clear();
    }

    /// Sets the separate single-byte output used by the reference's WRITE path.
    pub fn set_byte_to_send(&mut self, byte: u8) {
        self.byte_to_send = byte;
    }

    /// Returns the most recently set single-byte output.
    pub fn byte_to_send(&self) -> u8 {
        self.byte_to_send
    }
}

/// Program loader responding to the reference's secondary command `0x64`.
#[derive(Debug, Default)]
pub struct Loader {
    program: Vec<u8>,
}

impl Loader {
    /// Copies the program bytes so later changes to the caller's data do not
    /// alter the loaded program.
    pub fn load_from(&mut self, bytes: &[u8]) {
        self.program = bytes.to_vec();
    }

    /// Handles a secondary command; only `0x64` supplies the loaded bytes.
    ///
    /// An unloaded loader supplies an empty input, which polls as `0xff` with
    /// EOI as in the reference. Other commands leave the buffers unchanged.
    pub fn call(&self, secondary: u8, buffers: &mut GpibBuffers) {
        if secondary == 0x64 {
            buffers.replace_input(self.program.clone());
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputState {
    Idle,
    Waiting,
    Ready,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TapeReadState {
    Idle,
    Pending,
    Aborted,
}

pub(crate) enum GpibError {
    CommandNotImplemented { address: u8, command: u8 },
    RtcClock(jiff::Error),
    UnsupportedTapeSelection(Vec<u8>),
    InvalidTapeHeader(Vec<u8>),
    TapeFileNotSelected,
    TapePositionOverflow,
}

// RTC.js pads the day and time fields, but not the year modulo 100.
fn rtc_timestamp(date: DateTime) -> Vec<u8> {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    format!(
        "{:02}-{}-{} {:02}:{:02}:{:02}:{:03}",
        date.day(),
        MONTHS[usize::from(date.month() as u8 - 1)],
        date.year() % 100,
        date.hour(),
        date.minute(),
        date.second(),
        date.millisecond(),
    )
    .into_bytes()
}

/// Board-side GPIB state. Control writes advance the outbound handshake;
/// reading the control port advances the inbound DAV/NDAC handshake.
pub(crate) struct GpibController {
    buffers: GpibBuffers,
    loader: Loader,
    tape: Tape,
    data_out: u8,
    data_in: u8,
    talk: bool,
    listen: bool,
    atn_out: bool,
    eoi_out: bool,
    nrfd_out: bool,
    ndac_out: bool,
    dav_out: bool,
    eoi_in: bool,
    dav_in: bool,
    ndac_in: bool,
    state: InputState,
    tape_read_state: TapeReadState,
    primary: u8,
    secondary: u8,
    rtc_clock: RtcClock,
}

impl Default for GpibController {
    fn default() -> Self {
        Self {
            buffers: GpibBuffers::default(),
            loader: Loader::default(),
            tape: Tape::default(),
            data_out: 0,
            data_in: 0,
            talk: false,
            listen: false,
            atn_out: false,
            eoi_out: false,
            nrfd_out: false,
            ndac_out: false,
            dav_out: false,
            eoi_in: false,
            dav_in: false,
            ndac_in: true,
            state: InputState::Idle,
            tape_read_state: TapeReadState::Idle,
            primary: 0,
            secondary: 0,
            rtc_clock: crate::host_clock::local_datetime,
        }
    }
}

impl GpibController {
    pub(crate) fn set_rtc_clock(&mut self, clock: RtcClock) {
        self.rtc_clock = clock;
    }

    pub(crate) fn flash_drive_directory(&self) -> Vec<FlashDriveFileInfo> {
        self.tape.flash_drive_directory()
    }

    pub(crate) fn write_flash_storage_archive<W: std::io::Write + std::io::Seek>(
        &self,
        destination: W,
    ) -> Result<W, String> {
        self.tape.write_flash_storage_archive(destination)
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn flash_drive_archive_file(&self, number: u8) -> Option<OwnedArchiveFile> {
        self.tape.flash_drive_archive_file(number)
    }

    pub(crate) fn import_flash_storage_archive<R: std::io::Read + std::io::Seek>(
        &mut self,
        source: R,
    ) -> Result<(), String> {
        self.tape.import_flash_storage_archive(source)?;
        self.abort_tape_transfer_after_storage_replacement();
        Ok(())
    }

    pub(crate) fn clear_flash_drive(&mut self) {
        self.tape.clear_flash_drive();
        self.abort_tape_transfer_after_storage_replacement();
    }

    fn abort_tape_transfer_after_storage_replacement(&mut self) {
        if !matches!(self.primary, 0x25 | 0x45) {
            return;
        }
        // Keep a pending talker response addressed until its replacement sentinel reaches EOI.
        let finish_pending_read = self.primary == 0x45
            && matches!(
                self.tape_read_state,
                TapeReadState::Pending | TapeReadState::Aborted
            );
        self.buffers.clear_input();
        self.buffers.clear_output();
        self.buffers.set_byte_to_send(0);
        self.data_in = 0;
        self.eoi_in = false;
        self.dav_in = false;
        if finish_pending_read {
            self.buffers.replace_input(vec![0xff]);
            self.state = if self.listen && !self.atn_out {
                InputState::Ready
            } else {
                InputState::Waiting
            };
            self.tape_read_state = TapeReadState::Aborted;
        } else {
            self.primary = 0;
            self.secondary = 0;
            self.state = InputState::Idle;
            self.tape_read_state = TapeReadState::Idle;
        }
    }

    pub(crate) fn export_flash_file(
        &self,
        number: u8,
    ) -> Result<FlashDriveFile, FlashDriveExportError> {
        self.tape.export_flash_file(number)
    }

    pub(crate) fn export_flash_file_name(
        &self,
        number: u8,
    ) -> Result<String, FlashDriveExportError> {
        self.tape.export_flash_file_name(number)
    }

    pub(crate) fn import_flash_files(
        &mut self,
        files: &[FlashDriveFile],
    ) -> Vec<Result<(), FlashDriveImportError>> {
        self.tape.import_flash_files(files)
    }

    pub(crate) fn load_program(&mut self, bytes: &[u8]) {
        self.loader.load_from(bytes);
        self.state = InputState::Waiting;
    }

    pub(crate) fn store_tape_program(
        &mut self,
        number: u8,
        bytes: &[u8],
    ) -> Result<(), TapeFileNumberError> {
        self.tape.store_program(number, bytes)
    }

    pub(crate) fn set_data_output(&mut self, value: u8) {
        self.data_out = value;
    }

    pub(crate) fn set_keyboard_eoi(&mut self, value: u8) {
        self.eoi_out = value & 0x10 != 0;
    }

    pub(crate) fn write_control_a(&mut self, value: u8) -> Result<(), GpibError> {
        match (value >> 3) & 7 {
            6 => self.dav_out = false,
            7 => self.dav_out = true,
            _ => return Ok(()),
        }
        self.process()
    }

    pub(crate) fn write_control_b(&mut self, value: u8) -> Result<(), GpibError> {
        match (value >> 3) & 7 {
            6 => {
                self.talk = false;
                self.listen = true;
            }
            7 => {
                self.talk = true;
                self.listen = false;
            }
            _ => return Ok(()),
        }
        self.process()
    }

    pub(crate) fn write_port_b(&mut self, value: u8) -> Result<(), GpibError> {
        self.ndac_out = value & 0x80 != 0;
        self.nrfd_out = value & 0x10 != 0;
        self.atn_out = value & 0x08 == 0;
        self.eoi_out = value & 0x01 != 0;
        self.process()
    }

    // The reference acknowledges outbound DAV only once until DAV is lowered.
    fn process(&mut self) -> Result<(), GpibError> {
        let result = if self.talk && self.dav_out && self.ndac_in {
            self.process_outbound()
        } else {
            Ok(())
        };
        if self.talk {
            self.ndac_in = !self.dav_out;
        }
        if self.listen && !self.atn_out && self.primary != 0 && self.secondary != 0 {
            self.state = InputState::Ready;
        }
        result
    }

    fn process_outbound(&mut self) -> Result<(), GpibError> {
        if self.tape_read_state == TapeReadState::Aborted {
            if self.atn_out {
                self.buffers.clear_input();
                self.tape_read_state = TapeReadState::Idle;
            } else {
                return Ok(());
            }
        }
        if self.atn_out {
            match self.data_out {
                0x21..=0x5f => {
                    self.primary = self.data_out;
                    if self.data_out == 0x5f {
                        if self.state != InputState::Idle {
                            self.state = InputState::Waiting;
                        }
                        self.primary = 0;
                        self.secondary = 0;
                    }
                }
                0x60..=0x7f => {
                    self.secondary = self.data_out;
                    if (0x21..0x3f).contains(&self.primary) {
                        self.buffers.clear_output();
                    } else if (0x41..0x5f).contains(&self.primary) {
                        self.dispatch()?;
                    }
                }
                _ => {}
            }
        } else if self.primary != 0 && self.secondary != 0 {
            if self.secondary == 0x6f {
                self.buffers.set_byte_to_send(self.data_out);
                self.dispatch()?;
            } else {
                self.buffers.append_output(self.data_out);
                if self.eoi_out {
                    self.dispatch()?;
                    self.buffers.clear_output();
                }
            }
        }
        Ok(())
    }

    // Other addresses have no device in the reference's default configuration.
    fn dispatch(&mut self) -> Result<(), GpibError> {
        let address = match self.primary {
            0x21..=0x3f => self.primary - 0x20,
            0x41..=0x5f => self.primary - 0x40,
            _ => return Ok(()),
        };
        let tape_read = address == 5
            && self.primary > 0x40
            && matches!(self.secondary, 0x64 | 0x69 | 0x6d | 0x6e | 0x71 | 0x73);
        match address {
            1 => self.loader.call(self.secondary, &mut self.buffers),
            2 if self.secondary == 0x6d => {
                let date = (self.rtc_clock)().map_err(GpibError::RtcClock)?;
                self.buffers.replace_input(rtc_timestamp(date));
            }
            5 => match self.secondary {
                0x61 => self
                    .tape
                    .save(&mut self.buffers, FileType::Ascii)
                    .map_err(|()| GpibError::TapeFileNotSelected)?,
                0x62 => self.tape.close(),
                0x64 => self.tape.old(&mut self.buffers),
                0x69 => self.buffers.replace_input(b"/Tape@5/".to_vec()),
                0x6c => self
                    .tape
                    .print(&mut self.buffers)
                    .map_err(|error| match error {
                        TapePrintError::FileNotSelected => GpibError::TapeFileNotSelected,
                        TapePrintError::PositionOverflow => GpibError::TapePositionOverflow,
                    })?,
                0x6d => self.tape.input(&mut self.buffers),
                0x6e => self.tape.read(&mut self.buffers),
                0x6f => self
                    .tape
                    .write(&self.buffers)
                    .map_err(|()| GpibError::TapeFileNotSelected)?,
                0x71 if self.primary > 0x40 => self.tape.bold(&mut self.buffers),
                0x71 => self
                    .tape
                    .save(&mut self.buffers, FileType::Binary)
                    .map_err(|()| GpibError::TapeFileNotSelected)?,
                0x73 if self.primary > 0x40 => self.tape.list(&mut self.buffers),
                0x73 => self
                    .tape
                    .write_header(&mut self.buffers)
                    .map_err(|error| match error {
                        TapeHeaderError::Invalid(bytes) => GpibError::InvalidTapeHeader(bytes),
                        TapeHeaderError::FileNotSelected => GpibError::TapeFileNotSelected,
                    })?,
                0x7b => self
                    .tape
                    .find(&mut self.buffers)
                    .map_err(GpibError::UnsupportedTapeSelection)?,
                _ => {
                    return Err(GpibError::CommandNotImplemented {
                        address,
                        command: self.secondary,
                    });
                }
            },
            _ => {}
        }
        if tape_read {
            self.tape_read_state = TapeReadState::Pending;
        }
        Ok(())
    }

    /// Polls only when the GPIB PIA's control data register is read.
    pub(crate) fn read_control_input(&mut self) -> u8 {
        if self.listen {
            if !self.ndac_out && self.nrfd_out && !self.dav_in && self.state == InputState::Ready {
                (self.data_in, self.eoi_in) = self.buffers.poll();
                if self.eoi_in {
                    self.state = InputState::Idle;
                    self.buffers.clear_input();
                    if self.tape_read_state == TapeReadState::Aborted {
                        self.primary = 0;
                        self.secondary = 0;
                    }
                    self.tape_read_state = TapeReadState::Idle;
                }
                self.dav_in = true;
            } else if self.ndac_out && self.dav_in {
                self.dav_in = false;
                self.eoi_in = false;
            }
        }
        (u8::from(self.ndac_in) << 7) | (u8::from(self.dav_in) << 6)
    }

    pub(crate) fn data_input(&self) -> u8 {
        self.data_in
    }

    pub(crate) fn eoi_input(&self) -> bool {
        self.eoi_in
    }
}
