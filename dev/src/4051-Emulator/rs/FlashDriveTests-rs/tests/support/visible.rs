//! Main-thread native presentation shared by the Flash Drive scenarios.
//! The session owns ROM execution and checks; this view renders its framebuffer,
//! live input, and result from a worker thread, and requests automatic close.
//! Native-window errors and early closing remain test failures.

use std::{
    cell::Cell,
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use eframe::egui;
use tek4051_emulator::emulation::CpuReferenceRenderer;
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

use super::session::{Outcome, Scenario, Session, TICK};
use super::worker::{SessionView, SessionWorker, Snapshot};

const RESULT_VISIBLE: Duration = Duration::from_secs(5);

fn framebuffer(snapshot: &Snapshot) -> egui::ColorImage {
    egui::ColorImage::from_rgb(
        [CpuReferenceRenderer::WIDTH, CpuReferenceRenderer::HEIGHT],
        &snapshot
            .pixels
            .iter()
            .flat_map(|&pixel| [0, pixel, 0])
            .collect::<Vec<_>>(),
    )
}

struct VisibleTest {
    scenario: &'static Scenario,
    imported_files: usize,
    view: SessionView,
    snapshot: Snapshot,
    texture: egui::TextureHandle,
    revision: u64,
    started: Instant,
    finished: Option<Instant>,
    result: Rc<Cell<Option<Outcome>>>,
}

impl eframe::App for VisibleTest {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if self.snapshot.outcome.is_none() {
            match self.view.snapshot() {
                Ok(snapshot) => self.snapshot = snapshot,
                Err(error) => {
                    eprintln!("FAIL: {error}");
                    self.snapshot.outcome = Some(Err(error));
                }
            }
        }
        if let Some(outcome) = &self.snapshot.outcome {
            if self.finished.is_none() {
                self.finished = Some(Instant::now());
                self.result.set(Some(outcome.clone()));
            }
            if self
                .finished
                .is_some_and(|finished| finished.elapsed() >= RESULT_VISIBLE)
            {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.revision != self.snapshot.revision {
            self.texture
                .set(framebuffer(&self.snapshot), egui::TextureOptions::LINEAR);
            self.revision = self.snapshot.revision;
        }
        egui::TopBottomPanel::top("status").show(context, |ui| {
            ui.heading(format!(
                "Rust {} scripted Flash Drive test",
                self.scenario.title
            ));
            ui.label(format!(
                "Imported: {} files | PC {:04X} | BUSY {} | I/O {} | {:?} elapsed",
                self.imported_files,
                self.snapshot.pc,
                self.snapshot.busy,
                self.snapshot.io,
                self.started.elapsed(),
            ));
            match &self.snapshot.outcome {
                Some(Ok(())) => {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "PASS - closing in five seconds");
                }
                Some(Err(error)) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("FAIL - {error}"));
                }
                None => {
                    ui.label(format!("Live input: {}", self.snapshot.typed));
                }
            }
        });
        egui::SidePanel::left("sequence")
            .min_width(310.0)
            .show(context, |ui| {
                ui.heading("Command sequence");
                for (index, step) in self.scenario.steps.iter().enumerate() {
                    let marker = if index < self.snapshot.step {
                        "PASS"
                    } else if index == self.snapshot.step {
                        "NOW"
                    } else {
                        "WAIT"
                    };
                    ui.label(format!("{marker}: {}", step.label));
                    if !step.keys.is_empty() {
                        ui.monospace(String::from_utf8_lossy(step.keys).replace('\r', " <RETURN>"));
                    }
                    ui.separator();
                }
                ui.label("Real ROM execution; no supplied file edits.");
                ui.label("Scripted view only; manual keyboard input is not routed.");
                ui.label("x1 typing, x100 execution waits.");
                ui.label("Result is frozen for five seconds before automatic close.");
            });
        egui::CentralPanel::default().show(context, |ui| {
            let size = egui::vec2(
                CpuReferenceRenderer::WIDTH as f32,
                CpuReferenceRenderer::HEIGHT as f32,
            );
            let available = ui.available_size();
            let scale = (available.x / size.x).min(available.y / size.y).max(0.0);
            ui.add(egui::Image::new(&self.texture).fit_to_exact_size(size * scale));
        });
        context.request_repaint_after(TICK);
    }
}

// What: Display and execute the selected unchanged Flash Drive flow in a native Rust window.
// Why: Unsupported commands must produce a real regression failure, not a mocked success or skip.
pub(super) fn run(name: &str) -> ExitCode {
    // Arrange: Import the selected original assets and create a main-thread native window.
    let session = match Session::new(name) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("FAIL: {error}");
            return ExitCode::FAILURE;
        }
    };
    let scenario = session.scenario;
    let imported_files = session.imported_files;
    let (worker, view) = match SessionWorker::start(session) {
        Ok(worker) => worker,
        Err(error) => {
            eprintln!("FAIL: {error}");
            return ExitCode::FAILURE;
        }
    };
    let snapshot = match view.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("FAIL: {error}");
            if let Err(error) = worker.finish() {
                eprintln!("FAIL: {error}");
            }
            return ExitCode::FAILURE;
        }
    };
    let started = worker.started;
    let result = Rc::new(Cell::new(None));
    let shared_result = Rc::clone(&result);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Tek4051 Rust - {} live test", scenario.title))
            .with_inner_size([1250.0, 950.0])
            .with_visible(true)
            .with_active(true),
        #[cfg(target_os = "macos")]
        event_loop_builder: Some(Box::new(|builder| {
            // The visible test must not inherit an agent application's activation policy.
            builder.with_activation_policy(ActivationPolicy::Regular);
            builder.with_activate_ignoring_other_apps(true);
        })),
        ..Default::default()
    };
    // Act: The worker types and checks the real-ROM sequence while the frame loop displays snapshots.
    let window_result = eframe::run_native(
        &format!("Tek4051 {} test", scenario.title),
        options,
        Box::new(move |context| {
            let texture = context.egui_ctx.load_texture(
                "simple-demo-framebuffer",
                framebuffer(&snapshot),
                egui::TextureOptions::LINEAR,
            );
            let revision = snapshot.revision;
            Ok(Box::new(VisibleTest {
                scenario,
                imported_files,
                view,
                snapshot,
                texture,
                revision,
                started,
                finished: None,
                result: shared_result,
            }))
        }),
    );
    let worker_result = worker.finish();
    if let Err(error) = &window_result {
        eprintln!("FAIL: native window failed: {error}");
    }
    if let Err(error) = &worker_result {
        eprintln!("FAIL: {error}");
    }
    if window_result.is_err() || worker_result.is_err() {
        return ExitCode::FAILURE;
    }
    // Assert: Only the completed sequence passes; failure or an early window close is unsuccessful.
    match result.take() {
        Some(Ok(())) => ExitCode::SUCCESS,
        Some(Err(error)) => {
            eprintln!("FAIL: {error}");
            ExitCode::FAILURE
        }
        None => {
            eprintln!(
                "FAIL: window closed before the {} sequence completed",
                scenario.title
            );
            ExitCode::FAILURE
        }
    }
}
