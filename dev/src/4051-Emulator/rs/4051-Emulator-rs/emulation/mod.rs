//! Shared application-level commands, publications and file-effect contracts.
//!
//! Native input is translated before entering these types. Snapshots carry
//! owned metadata and shared grayscale pixels, not widgets or mutable hardware.
//! File effects describe host work and its notices; they do not open files.
//! The session owns execution; hosts supply time and execute returned effects.
//! The CPU reference renderer consumes ordered machine display operations.
//! The optional WebGPU module rasterizes emulation-owned vector geometry offscreen.
//! This module creates no transport, window or second session state.

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

pub mod display;
#[cfg(feature = "webgpu")]
pub mod extended_webgpu;
pub mod input;
pub mod keyboard;
pub mod runtime;
#[cfg(feature = "webgpu")]
pub(crate) mod vector_scene;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub use display::{CpuReferenceRenderer, DisplayGranularity};

mod cursor_readiness;

use crate::flash_drive_import::FlashImportSelection;
use crate::machine::{
    MachineIndicators,
    cpu::CpuRegisters,
    gpib::{FlashDriveFile, FlashDriveFileInfo},
};

use self::input::{AutoLoadState, InputFrame};

mod session;
pub use session::{EmulationState, TICK};

/// Supported CPU speed multipliers, indexed by `Command::Speed` and `Snapshot::speed_index`.
pub const SPEED_FACTORS: [u32; 7] = [1, 2, 5, 10, 20, 50, 100];

/// Ordered requests to the emulation owner, after native input interpretation.
pub enum Command {
    Run(bool),
    Reset,
    Speed(usize),
    Granularity(DisplayGranularity),
    CapsLock(bool),
    PromptReady { ready: bool, request: u64 },
    AutoLoad,
    Launch(u8),
    Input(Box<InputFrame>),
    BeginFileOperation(u64),
    Modal(bool),
    ImportFiles(Vec<FlashDriveFile>),
    ImportSelectedFiles(FlashImportSelection),
    ImportArchive(PathBuf),
    ExportFile { number: u8, path: PathBuf },
    ExportArchive(PathBuf),
    RequestScreenCopy,
    SaveScreen { id: u64, path: PathBuf },
    CancelScreenCopy(u64),
}

/// Execute on the session owner before another command or time advancement.
/// Archive effects use that owner's suspended machine; numbered exports and
/// screen saves already own the selected event-time data.
pub enum FileEffect {
    ImportArchive(PathBuf),
    ExportFile {
        number: u8,
        path: PathBuf,
        file: FlashDriveFile,
    },
    ExportArchive(PathBuf),
    SaveScreen {
        path: PathBuf,
        pixels: Arc<[u8]>,
    },
}

/// Operation-specific completion notices; the boolean is true for an error.
/// The session retains notice ordering and screen-overflow policy, not host I/O.
pub enum FileOutcome {
    Import((bool, String)),
    Export((bool, String)),
    Screen((bool, String)),
}

/// Published session values, with no mutable reference back to the owner.
/// The grayscale pixels remain the CPU reference frame; native frontends may
/// present the optional GPU texture while screen-copy requests carry pixels
/// from the active renderer.
#[derive(Clone)]
pub struct Snapshot {
    pub pixels: Arc<[u8]>,
    pub revision: u64,
    pub indicators: MachineIndicators,
    pub registers: CpuRegisters,
    pub directory: Vec<FlashDriveFileInfo>,
    pub export_names: HashMap<u8, Result<String, String>>,
    pub granularity: DisplayGranularity,
    pub running: bool,
    pub caps_lock: bool,
    pub speed_index: usize,
    pub settings_error: Option<String>,
    pub runtime_error: Option<String>,
    /// Explains why an optional renderer fell back to the CPU reference.
    pub render_error: Option<String>,
    /// True while the optional WebGPU renderer owns the presented frame.
    pub webgpu_active: bool,
    /// True while the Extended WebGPU renderer owns the presented frame.
    pub extended_webgpu_active: bool,
    /// True after a BASIC cursor or INPUT-prompt glyph, until output or a command transition clears it.
    pub basic_prompt_ready: bool,
    /// Echoes the latest accepted legacy readiness request.
    pub prompt_request: u64,
    /// Explains why host key presses cannot be delivered while emulation is stopped.
    pub input_notice: Option<String>,
    pub program_launch_file: Option<u8>,
    pub program_launch_notice: Option<(bool, String)>,
    pub auto_load_error: Option<String>,
    pub auto_load_state: AutoLoadState,
    pub import_notices: Vec<(bool, String)>,
    pub export_notice: Option<(bool, String)>,
    pub screen_copy_notices: Vec<(bool, String)>,
    pub screen_copy_requests: Vec<(u64, Arc<[u8]>)>,
    pub coalesced_time: Duration,
    /// The owner's modal suspension state, independent of running and typing.
    pub busy: bool,
    /// Echoes the applied file-operation request used before opening a chooser.
    pub file_operation: u64,
}
