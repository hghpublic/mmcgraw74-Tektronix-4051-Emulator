//! Platform-neutral display operations emitted by the machine.
//!
//! The bus samples the display PIA latches and records operations in machine
//! order. The emulation layer consumes them to maintain the rendered display;
//! this module contains no rasterizer or presentation resources.

/// Display PIA values sampled when a display operation is issued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayControl {
    pub x_character: u8,
    pub y_character: u8,
    pub vector_0: bool,
    pub ven_1: bool,
    pub x: u16,
    pub y: u16,
}

/// A display command triggered by a PIA read or buffer-clock write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenCommand {
    BufferClock,
    StartOfText,
    AlphaDot,
}

/// Ordered machine-to-emulation display output.
///
/// Screen copies are requests, not pixel data: the emulation renderer captures
/// its current frame when it consumes the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayOperation {
    ScreenCommand {
        command: ScreenCommand,
        control: DisplayControl,
    },
    BeginErase,
    CompleteErase,
    ScreenCopy,
}
