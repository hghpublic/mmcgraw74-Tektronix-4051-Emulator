//! Machine hardware, host timing, neutral input and reusable emulation sessions.
//!
//! The machine connects the processor to its bus and peripherals; the native
//! application supplies elapsed time and presents the resulting framebuffer.
//! Disable default features for library builds without native window/dialog/PNG dependencies.
#![forbid(unsafe_code)]

// Shared native test sources retain the same library imports as the executable.
#[cfg(all(test, feature = "native"))]
extern crate self as tek4051_emulator;

pub mod emulation;
pub use emulation::input as emulation_input;
mod flash_drive;
#[path = "flash_drive/import.rs"]
pub mod flash_drive_import;
#[path = "host/clock.rs"]
pub mod host_clock;
pub use emulation::runtime as host_runtime;
pub mod machine;
#[cfg(feature = "wasm")]
#[path = "platforms/wasm/mod.rs"]
pub mod wasm;
