//! UI-independent CPU, machine, display, and host-timing components.
//!
//! The machine connects the processor to its bus and peripherals; the native
//! application supplies elapsed time and presents the resulting framebuffer.
#![forbid(unsafe_code)]

pub mod cpu;
pub mod display;
pub mod host_runtime;
pub mod machine;
mod machine_bus;
pub mod pia;
pub mod rom;
