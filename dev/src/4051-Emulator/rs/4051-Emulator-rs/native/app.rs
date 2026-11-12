//! Starts the native window and its independently owned emulation worker.
//!
//! The UI submits captured input and renders immutable, precolored snapshots without
//! waiting for CPU or file work. The worker owns timing and mutable machine state;
//! device behavior remains in the library. Execution
//! begins when the user selects Start and pauses without accumulating wall time.
//! Native Flash Drive imports read host files or ZIP entries outside the core and
//! report each failure while keeping successful imports. Modal time advances only
//! erase callbacks.
//! Whole-storage ZIP import confirms replacement, validates before commit, and aborts an addressed tape transfer.
//! Running-only Auto Load and directory launches queue FIND/OLD/RUN commands.
//! Directory launches require a running runtime and machine-detected BASIC prompt readiness.
//! Interruptions cancel unsent keys rather than reporting them as sent.
//! Single-file Flash Drive export writes a canonical snapshot through native Save As.
//! Whole-storage exports use alternating JSON-header and content ZIP entries.
//! An optional directory panel shows canonical metadata and launches ASCII programs.

#[path = "../gui/front_panel.rs"]
mod front_panel;
#[path = "../gui/app.rs"]
mod gui;
#[path = "../host/dialogs.rs"]
mod host_dialogs;
#[path = "../host/import.rs"]
mod host_import;
#[path = "../host/io.rs"]
mod host_io;
#[path = "../gui/keyboard_input.rs"]
mod keyboard_input;
#[path = "backend.rs"]
mod native_backend;
use gui::Tek4051App;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../emulation/tests.rs"]
mod emulation_tests;

#[cfg(test)]
#[path = "test_support.rs"]
mod test_support;
#[cfg(test)]
use test_support::{
    FlashExportRequest, FlashImportDialogs, import_flash_storage_archive_dialog, speed_control,
};

use std::time::{Duration, Instant};

use self::host_import::FlashImportSelection;
use eframe::egui;
use front_panel::FrontPanel;
use keyboard_input::KeyboardInput;
#[cfg(test)]
use tek4051_emulator::emulation::EmulationState;
use tek4051_emulator::emulation::{CpuReferenceRenderer, DisplayGranularity};
use tek4051_emulator::emulation_input::AutoLoadState;
use tek4051_emulator::machine::Machine;
#[cfg(test)]
use tek4051_emulator::machine::gpib::FlashDriveFile;

#[cfg_attr(
    test,
    expect(
        dead_code,
        reason = "Headless fixtures compile startup but do not launch the native event loop"
    )
)]
pub(super) fn run() -> eframe::Result {
    #[cfg(feature = "webgpu")]
    {
        match run_with_renderer(eframe::Renderer::Wgpu, None) {
            Ok(()) => Ok(()),
            Err(error) => {
                let notice = format!(
                    "WebGPU window initialization failed; using the CPU reference renderer: {error}"
                );
                eprintln!("{notice}");
                run_with_renderer(eframe::Renderer::Glow, Some(notice))
            }
        }
    }
    #[cfg(not(feature = "webgpu"))]
    {
        run_with_renderer(eframe::Renderer::default())
    }
}

fn run_with_renderer(
    renderer: eframe::Renderer,
    #[cfg(feature = "webgpu")] fallback_notice: Option<String>,
) -> eframe::Result {
    let mut machine = Machine::default();
    machine.set_rtc_clock(tek4051_emulator::host_clock::local_datetime);
    machine
        .reset()
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?;
    let mut backend = native_backend::WorkerBackend::start(machine)
        .map_err(|error| eframe::Error::AppCreation(error.into()))?;
    let shutdown = backend.shutdown_handle();
    let join = backend.take_join_handle();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tektronix 4051 Emulator")
            .with_inner_size([900.0, 720.0])
            .with_min_inner_size([480.0, 360.0]),
        renderer,
        ..Default::default()
    };

    #[cfg(feature = "webgpu")]
    let app_notice = fallback_notice.clone();
    let window_result = eframe::run_native(
        "Tektronix 4051 Emulator",
        options,
        Box::new(move |context| {
            let app = Tek4051App::with_backend(context, backend)?;
            #[cfg(feature = "webgpu")]
            let app = {
                let mut app = app;
                if let Some(notice) = app_notice {
                    app.set_webgpu_notice(notice);
                }
                app
            };
            Ok(Box::new(app))
        }),
    );
    // Joining is deliberately outside the native event loop, including startup failure.
    shutdown.request_shutdown();
    if let Some(join) = join {
        let worker_result = join.join().map_err(|_| {
            eframe::Error::AppCreation("emulation worker panicked during shutdown".into())
        })?;
        worker_result.map_err(|error| eframe::Error::AppCreation(error.into()))?;
    }
    window_result
}
