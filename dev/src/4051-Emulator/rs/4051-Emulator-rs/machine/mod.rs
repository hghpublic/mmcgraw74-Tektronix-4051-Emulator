//! Tektronix 4051 hardware: CPU, memory/bus, PIAs, ROMs, display operations and device I/O.
//!
//! Component modules support direct CPU/display-operation/device consumers and
//! tests; `Machine` connects them through a private bus. The emulation layer
//! consumes display operations and owns rendering; host timing and native input,
//! widgets and worker orchestration remain outside this module.
//!
//! Interrupt sources are sampled before each instruction. NMI takes priority;
//! held NMI is serviced once until cleared; a masked IRQ is reconsidered when
//! the CPU's interrupt mask is removed.

mod bus;
pub mod cpu;
pub mod display;
pub mod gpib;
pub mod pia;
pub mod rom;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use self::bus::MachineBus;
use self::cpu::{Cpu6800, CpuError, CpuRegisters, MemoryBus, StepOutcome};
use self::display::DisplayOperation;
use self::gpib::{
    FlashDriveExportError, FlashDriveFile, FlashDriveFileInfo, FlashDriveImportError, RtcClock,
    TapeFileNumberError,
};

pub use self::bus::{KeyboardModifier, MachineBusError};
pub use self::pia::{PiaControlLine, PiaId, PiaPort};

/// Front-panel indicators driven by the keyboard PIA output latch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MachineIndicators {
    pub busy: bool,
    pub io: bool,
    pub break_active: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BasicKeyboardLatch {
    Unavailable,
    Released,
    Pressed(u8),
}

/// A UI-independent Tektronix 4051 machine core with configurable RTC sampling.
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

    /// Starts the display's full-bright erase flash.
    ///
    /// The host completes the erase by calling `display_ready` after 500 ms.
    pub fn erase_display(&mut self) {
        self.bus.erase_display();
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
                if self.cpu.registers.ccr & cpu::INTERRUPT_MASK == 0 {
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
        #[cfg(test)]
        self.bus.set_cpu_instruction_active(true);
        let outcome = self.cpu.step(&mut self.bus);
        #[cfg(test)]
        self.bus.set_cpu_instruction_active(false);
        outcome
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

    #[cfg(test)]
    pub(crate) fn basic_keyboard_latch(&self) -> BasicKeyboardLatch {
        self.bus.basic_keyboard_latch()
    }

    pub fn write_byte(&mut self, address: u16, value: u8) -> Result<(), MachineBusError> {
        self.bus.write_byte(address, value)
    }

    /// Copies program bytes into the GPIB Loader at its default address 1.
    ///
    /// This does not type a load command or start the program.
    pub fn load_program(&mut self, bytes: &[u8]) {
        self.bus.load_program(bytes);
    }

    /// Replaces the RTC sampler without changing buffered data or handshake state.
    ///
    /// By default, each address-2 command `0x6d` samples host-local time.
    /// This callback is invoked at that same dispatch point, not at installation
    /// or while receiving bytes. CPU reset retains it; errors use the existing
    /// `MachineBusError::RtcClockUnavailable` path.
    pub fn set_rtc_clock(&mut self, clock: RtcClock) {
        self.bus.set_rtc_clock(clock);
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn stage_editor_program(&mut self, bytes: &[u8]) -> u8 {
        self.bus.stage_editor_program(bytes)
    }

    pub(crate) fn clear_editor_program(&mut self) {
        self.bus.clear_editor_program();
    }

    /// Copies an ASCII program into the default tape at file number 1 through 254.
    ///
    /// FIND selects it for OLD; this does not type commands or start the program.
    pub fn store_tape_program(
        &mut self,
        number: u8,
        bytes: &[u8],
    ) -> Result<(), TapeFileNumberError> {
        self.bus.store_tape_program(number, bytes)
    }

    /// Imports Flash Drive files, returning one outcome per input in the same order.
    ///
    /// Valid files replace matching numbers without clearing other storage. The first
    /// stored number is selected; FIND is still required to refresh the OLD snapshot.
    /// Invalid files do not prevent later imports. No commands are typed or executed.
    pub fn import_flash_files(
        &mut self,
        files: &[FlashDriveFile],
    ) -> Vec<Result<(), FlashDriveImportError>> {
        self.bus.import_flash_files(files)
    }

    /// Copies stored metadata in ascending file-number order without copying file contents.
    ///
    /// Reads leave selection, FIND snapshots, transfer cursors/buffers and pending writes unchanged.
    /// Description units are retained verbatim so callers can report invalid UTF-16 explicitly.
    pub fn flash_drive_directory(&self) -> Vec<FlashDriveFileInfo> {
        self.bus.flash_drive_directory()
    }

    /// Writes a complete, numerically ordered Flash Drive archive to a seekable writer.
    ///
    /// Each stored file becomes a JSON header entry followed by its unchanged content entry.
    /// The archive is streamed from storage without changing selection or transfer state.
    pub fn write_flash_storage_archive<W: std::io::Write + std::io::Seek>(
        &self,
        destination: W,
    ) -> Result<W, String> {
        self.bus.write_flash_storage_archive(destination)
    }

    #[cfg(feature = "wasm")]
    pub(crate) fn flash_drive_archive_file(
        &self,
        number: u8,
    ) -> Option<crate::flash_drive::archive::OwnedArchiveFile> {
        self.bus.flash_drive_archive_file(number)
    }

    /// Replaces the complete Flash Drive store from a web-format ZIP archive.
    ///
    /// Entries alternate between JSON metadata headers and raw file contents; their names
    /// are ignored. The whole archive is validated before storage changes, duplicate file
    /// numbers use the last pair, and an empty archive clears the store. A successful
    /// replacement selects the lowest stored number, clears the old OLD snapshot, and
    /// resets the cursor and pending WRITE clear. Compressed size, entry count, per-entry
    /// expansion, and total expansion are bounded. Any currently addressed tape transfer
    /// is aborted; a pending read receives the empty-input sentinel with EOI.
    pub fn import_flash_storage_archive<R: std::io::Read + std::io::Seek>(
        &mut self,
        source: R,
    ) -> Result<(), String> {
        self.bus.import_flash_storage_archive(source)
    }

    /// Clears stored files and resets the associated tape selection and transfer state.
    ///
    /// An addressed tape transfer is aborted; a pending read is completed with the empty-input sentinel.
    pub fn clear_flash_drive(&mut self) {
        self.bus.clear_flash_drive();
    }

    /// Copies one canonical stored file with its numbered Flash Drive filename.
    ///
    /// Reads do not change FIND/OLD snapshots, selection, cursors, or pending transfers.
    /// Invalid numbers, missing files, and unrepresentable host filenames report errors.
    pub fn export_flash_file(&self, number: u8) -> Result<FlashDriveFile, FlashDriveExportError> {
        self.bus.export_flash_file(number)
    }

    /// Returns the canonical export filename without copying the stored payload.
    ///
    /// Uses the same validation as file export and leaves transfer state unchanged.
    pub fn export_flash_file_name(&self, number: u8) -> Result<String, FlashDriveExportError> {
        self.bus.export_flash_file_name(number)
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

    /// Returns BUSY, I/O, and BREAK levels from the keyboard output latch.
    pub fn indicators(&self) -> MachineIndicators {
        self.bus.indicators()
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

    /// Takes display operations produced since the previous call, in machine order.
    ///
    /// Emulation owners should drain these operations regularly and apply them to
    /// the emulator renderer. The machine retains operations until this method drains them.
    pub fn take_display_operations(&mut self) -> Vec<DisplayOperation> {
        self.bus.take_display_operations()
    }

    pub(crate) fn take_display_erase_requests(&mut self) -> usize {
        self.bus.take_display_erase_requests()
    }

    pub(crate) fn complete_display_erase(&mut self) {
        self.bus.complete_display_erase();
    }

    /// Tracks display busy state until an erase completion signals readiness.
    /// An earlier completion can clear it while another erase timeout remains scheduled.
    pub fn display_erase_pending(&self) -> bool {
        self.bus.display_erase_pending()
    }

    /// Returns the current ROM selector and expander address field.
    pub fn bank_selection(&self) -> (u8, u8) {
        self.bus.bank_selection()
    }
}
