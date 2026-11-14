//! Runs the native eframe window, host timing, keyboard input, and display presentation.
//!
//! The UI forwards host keys to the UI-independent machine and presents framebuffer
//! revisions; emulator timing and device behavior remain in the library. Execution
//! begins when the user selects Start and pauses without accumulating wall time.

mod front_panel;
mod keyboard_input;

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;

use std::{
    fs::File,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use eframe::egui;
use front_panel::FrontPanel;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use keyboard_input::KeyboardInput;
use tek4051_emulator::{
    display::{DisplayGranularity, DisplayModel},
    host_runtime::HostRuntime,
    machine::Machine,
};

const SPEED_FACTORS: [u32; 7] = [1, 2, 5, 10, 20, 50, 100];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tektronix 4051 Emulator")
            .with_inner_size([900.0, 720.0])
            .with_min_inner_size([480.0, 360.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Tektronix 4051 Emulator",
        options,
        Box::new(|context| {
            let mut machine = Machine::default();
            machine.reset()?;
            Ok(Box::new(Tek4051App::new(context, machine)))
        }),
    )
}

struct Tek4051App {
    machine: Machine,
    runtime: HostRuntime,
    display_texture: egui::TextureHandle,
    display_revision: u64,
    keyboard_input: KeyboardInput,
    front_panel: FrontPanel,
    caps_lock: bool,
    running: bool,
    speed_index: usize,
    settings_error: Option<String>,
    last_modifiers: egui::Modifiers,
    last_update: Instant,
    runtime_error: Option<String>,
    screen_copy_notices: Vec<(bool, String)>,
}

impl Tek4051App {
    fn new(context: &eframe::CreationContext<'_>, mut machine: Machine) -> Self {
        let display = machine.display();
        let display_texture = context.egui_ctx.load_texture(
            "tek4051-display",
            framebuffer_image(display),
            egui::TextureOptions::NEAREST,
        );
        let display_revision = display.revision();
        let mut runtime = HostRuntime::start(&mut machine);
        runtime.pause();

        Self {
            machine,
            runtime,
            display_texture,
            display_revision,
            keyboard_input: KeyboardInput::default(),
            front_panel: FrontPanel::default(),
            caps_lock: false,
            running: false,
            speed_index: 0,
            settings_error: None,
            last_modifiers: egui::Modifiers::NONE,
            last_update: Instant::now(),
            runtime_error: None,
            screen_copy_notices: Vec::new(),
        }
    }

    fn advance_machine(&mut self, now: Instant, modifiers: egui::Modifiers) {
        let elapsed = now.saturating_duration_since(self.last_update);
        self.last_update = now;
        if self.runtime_error.is_none()
            && let Err(error) = self.front_panel.advance(
                elapsed,
                &mut self.keyboard_input,
                &mut self.machine,
                &mut self.runtime,
                modifiers,
            )
        {
            self.running = false;
            self.runtime_error = Some(error.to_string());
        }
    }

    fn update_display_texture(&mut self) {
        let display = self.machine.display();
        if display.revision() != self.display_revision {
            self.display_texture
                .set(framebuffer_image(display), egui::TextureOptions::NEAREST);
            self.display_revision = display.revision();
        }
    }
}

impl Tek4051App {
    fn update_at(&mut self, context: &egui::Context, now: Instant) {
        self.update_at_with_save(context, now, &mut save_screen_dialog, &mut Instant::now);
    }

    fn update_at_with_save(
        &mut self,
        context: &egui::Context,
        now: Instant,
        save: &mut impl FnMut(&[u8]) -> Result<Option<PathBuf>, String>,
        clock: &mut impl FnMut() -> Instant,
    ) {
        self.advance_machine(now, self.last_modifiers);
        self.update_display_texture();
        let (mut copies, dropped_copies) = self.machine.take_screen_copies();
        let mut copy_requested = false;

        egui::TopBottomPanel::top("display_controls").show(context, |ui| {
            ui.horizontal(|ui| {
                let run_label = if self.running { "Pause" } else { "Start" };
                if ui
                    .add_enabled(self.runtime_error.is_none(), egui::Button::new(run_label))
                    .clicked()
                {
                    if self.running {
                        self.runtime.pause();
                        self.running = false;
                    } else {
                        match self.runtime.resume() {
                            Ok(()) => self.running = true,
                            Err(error) => {
                                self.runtime_error = Some(error.to_string());
                            }
                        }
                    }
                }
                ui.label("Display granularity:");
                let mut granularity = self.machine.display().granularity();
                ui.selectable_value(&mut granularity, DisplayGranularity::Fine, "Fine");
                ui.selectable_value(&mut granularity, DisplayGranularity::Coarse, "Coarse");
                ui.selectable_value(&mut granularity, DisplayGranularity::Mixed, "Mixed");
                if granularity != self.machine.display().granularity() {
                    self.machine.set_display_granularity(granularity);
                }
                ui.label("CPU speed:");
                speed_control(
                    ui,
                    &mut self.runtime,
                    &mut self.speed_index,
                    &mut self.settings_error,
                );
                if ui.checkbox(&mut self.caps_lock, "Caps Lock").changed() {
                    self.machine.toggle_caps_lock();
                }
                if let Some(error) = &self.runtime_error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("Emulation stopped: {error}"),
                    );
                } else {
                    ui.label("Reset and storage are not connected.");
                }
            });
            if let Some(error) = &self.settings_error {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("Speed setting: {error}"));
            }
            for (error, notice) in &self.screen_copy_notices {
                if *error {
                    ui.colored_label(egui::Color32::LIGHT_RED, notice);
                } else {
                    ui.label(notice);
                }
            }
            copy_requested = self.front_panel.show(ui);
        });

        egui::CentralPanel::default().show(context, |ui| {
            let available_size = ui.available_size();
            let native_size = egui::vec2(
                self.machine.display().width() as f32,
                self.machine.display().height() as f32,
            );
            let scale = (available_size.x / native_size.x)
                .min(available_size.y / native_size.y)
                .max(0.0);
            ui.add(
                egui::Image::new(&self.display_texture)
                    .fit_to_exact_size(native_size * scale)
                    .maintain_aspect_ratio(true),
            );
        });

        self.keyboard_input
            .handle_frame(&mut self.machine, context, &mut self.front_panel);
        if copy_requested {
            copies.push(self.machine.display().pixels().to_vec());
        }
        if !copies.is_empty() || dropped_copies != 0 {
            self.screen_copy_notices.clear();
            if dropped_copies != 0 {
                self.screen_copy_notices.push((
                    true,
                    format!("At least {dropped_copies} screen copy request(s) could not be saved: queue full"),
                ));
            }
            for pixels in copies {
                self.screen_copy_notices.push(match save(&pixels) {
                    Ok(Some(path)) => (false, format!("Screen copy saved: {}", path.display())),
                    Ok(None) => (false, "Screen copy cancelled".to_owned()),
                    Err(error) => (true, format!("Screen copy failed: {error}")),
                });
            }
            // A native modal dialog must not add its open time to emulation.
            self.last_update = clock();
            context.request_repaint();
        }
        self.last_modifiers = context.input(|input| {
            if input.focused {
                input.modifiers
            } else {
                egui::Modifiers::NONE
            }
        });
        if self.runtime_error.is_none() {
            context.request_repaint_after(Duration::from_millis(10));
        }
    }
}

fn save_screen_dialog(pixels: &[u8]) -> Result<Option<PathBuf>, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Save screen copy")
        .set_file_name("screen.png")
        .add_filter("PNG image", &["png"])
        .save_file()
    else {
        return Ok(None);
    };
    save_screen(&path, pixels).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(Some(path))
}

fn save_screen(path: &Path, pixels: &[u8]) -> Result<(), image::ImageError> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &intensity in pixels {
        rgba.extend_from_slice(&[0, intensity, 0, u8::MAX]);
    }
    PngEncoder::new(File::create(path)?).write_image(
        &rgba,
        DisplayModel::WIDTH as u32,
        DisplayModel::HEIGHT as u32,
        ExtendedColorType::Rgba8,
    )
}

impl eframe::App for Tek4051App {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_at(context, Instant::now());
    }
}

fn speed_control(
    ui: &mut egui::Ui,
    runtime: &mut HostRuntime,
    speed_index: &mut usize,
    settings_error: &mut Option<String>,
) -> egui::Response {
    let response = ui.button(format!("x{}", SPEED_FACTORS[*speed_index]));
    if response.clicked() {
        let next_index = (*speed_index + 1) % SPEED_FACTORS.len();
        match runtime.set_speed_factor(f64::from(SPEED_FACTORS[next_index])) {
            Ok(()) => {
                *speed_index = next_index;
                *settings_error = None;
            }
            Err(error) => *settings_error = Some(error.to_string()),
        }
    }
    response
}

fn framebuffer_image(display: &DisplayModel) -> egui::ColorImage {
    let mut rgba = Vec::with_capacity(display.pixels().len() * 4);
    for intensity in display.pixels() {
        rgba.extend_from_slice(&[0, *intensity, 0, u8::MAX]);
    }
    egui::ColorImage::from_rgba_unmultiplied([display.width(), display.height()], &rgba)
}
