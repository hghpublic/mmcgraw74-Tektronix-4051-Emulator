use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use tek4051_emulator::{display::DisplayModel, host_runtime::HostRuntime, machine::Machine};

use super::{Tek4051App, save_screen, speed_control};

fn app_frame(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
        events,
        ..Default::default()
    };
    context
        .run(input, |context| app.update_at(context, now))
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some((text.galley.job.text.clone(), text.pos + Vec2::splat(2.0))),
            _ => None,
        })
        .collect()
}

fn speed_control_frame(
    context: &egui::Context,
    runtime: &mut HostRuntime,
    speed_index: &mut usize,
    settings_error: &mut Option<String>,
    events: Vec<Event>,
) -> (Rect, Vec<String>) {
    let mut button_rect = Rect::NOTHING;
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 100.0))),
        events,
        ..Default::default()
    };
    let output = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            button_rect = speed_control(ui, runtime, speed_index, settings_error).rect;
        });
    });
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    (button_rect, text)
}

fn click_speed_control(
    context: &egui::Context,
    runtime: &mut HostRuntime,
    speed_index: &mut usize,
    settings_error: &mut Option<String>,
    position: Pos2,
) {
    for pressed in [true, false] {
        speed_control_frame(
            context,
            runtime,
            speed_index,
            settings_error,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

// What: Click through all reference speed settings and execute at each selected speed.
// Why: Every displayed setting must apply its corresponding CPU budget and wrap back to x1.
#[test]
fn speed_button_updates_runtime_and_wraps_after_x100() {
    // Arrange: A long NOP loop yields a different PC for every reference instruction budget.
    let context = egui::Context::default();
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let start = 0x0100;
    for offset in 0..30_000 {
        machine.write_byte(start + offset, 0x01).unwrap();
    }

    machine.write_byte(start + 30_000, 0x7e).unwrap();
    machine.write_byte(start + 30_001, 0x01).unwrap();
    machine.write_byte(start + 30_002, 0x00).unwrap();
    machine.cpu_mut().registers.pc = start;
    let mut runtime = HostRuntime::start(&mut machine);
    let mut speed_index = 0;
    let mut settings_error = None;
    let (button, initial_text) = speed_control_frame(
        &context,
        &mut runtime,
        &mut speed_index,
        &mut settings_error,
        vec![],
    );
    assert_eq!(initial_text, ["x1"]);

    // Act: Select each speed, repaint its label, and run one 10 ms CPU interval.
    for (expected_label, expected_pc_offset) in [
        ("x2", 4_528),
        ("x5", 11_318),
        ("x10", 22_636),
        ("x20", 15_271),
        ("x50", 23_177),
        ("x100", 16_352),
        ("x1", 2_264),
    ] {
        click_speed_control(
            &context,
            &mut runtime,
            &mut speed_index,
            &mut settings_error,
            button.center(),
        );
        let (_, text) = speed_control_frame(
            &context,
            &mut runtime,
            &mut speed_index,
            &mut settings_error,
            vec![],
        );
        machine.cpu_mut().registers.pc = start;
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();

        // Assert: The visible label and executed budget match this reference speed.
        assert_eq!(text, [expected_label]);
        assert_eq!(machine.registers().pc, start + expected_pc_offset);
        assert_eq!(settings_error, None);
    }
}

// What: Change speed after an elapsed interval in the native application update.
// Why: A new speed must not retroactively change the prior interval's instruction budget.
#[test]
fn speed_click_applies_after_previous_elapsed_interval() {
    // Arrange: Start the app on enough NOPs to observe x1 followed by x2.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for offset in 0..7_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }

    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    let text = app_frame(&mut app, &context, start, vec![]);
    let position = text.iter().find(|(text, _)| text == "x1").unwrap().1;

    // Act: Hold the speed button, then release it after 10 ms has elapsed.
    for (pressed, now) in [(true, start), (false, start + Duration::from_millis(10))] {
        app_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }

    // Assert: The prior interval used x1; the next interval uses the selected x2.
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264);
    app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(20),
        vec![],
    );
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264 + 4_528);
}

// What: Toggle Caps Lock from the native UI and sample the keyboard PIA input.
// Why: The visible control must change the line consumed by the base ROM.
#[test]
fn caps_lock_checkbox_toggles_the_keyboard_tty_line() {
    // Arrange: Select keyboard data registers and locate the rendered checkbox label.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, position)| (text == "Caps Lock").then_some(position))
        .unwrap();

    // Act: Turn the control on, sample the ROM-facing bit, then turn it off.
    for pressed in [true, false] {
        app_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    app.machine.keyboard_press(b'B');
    let caps_lock_on = app.machine.read_byte(0x87a8).unwrap();
    app.machine.keyboard_release();
    for pressed in [true, false] {
        app_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    app.machine.keyboard_press(b'B');
    let caps_lock_off = app.machine.read_byte(0x87a8).unwrap();

    // Assert: Each checkbox transition flips the Caps Lock bit in the key byte.
    assert_eq!(caps_lock_on, 0x42);
    assert_eq!(caps_lock_off, 0xc2);
}

// What: Encode a full display snapshot and report an inaccessible destination.
// Why: Screen copies must preserve green intensities and surface file failures.
#[test]
fn screen_copy_png_preserves_pixels_and_reports_file_errors() {
    // Arrange: Prepare distinct pixels at both ends of a full-size snapshot.
    let mut pixels = vec![0; DisplayModel::WIDTH * DisplayModel::HEIGHT];
    pixels[0] = 200;
    *pixels.last_mut().unwrap() = 254;

    // Act: Save and decode a PNG, then try writing to a path that is a directory.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("screen.png");
    save_screen(&path, &pixels).unwrap();
    let decoded = image::open(&path).unwrap().to_rgba8();
    let error = save_screen(Path::new("."), &pixels).unwrap_err();

    // Assert: PNG dimensions and RGBA pixels match; destination errors are returned.
    assert_eq!(
        decoded.dimensions(),
        (DisplayModel::WIDTH as u32, DisplayModel::HEIGHT as u32)
    );
    assert_eq!(decoded.get_pixel(0, 0).0, [0, 200, 0, 255]);
    assert_eq!(
        decoded
            .get_pixel(
                DisplayModel::WIDTH as u32 - 1,
                DisplayModel::HEIGHT as u32 - 1
            )
            .0,
        [0, 254, 0, 255]
    );
    assert!(matches!(error, image::ImageError::IoError(_)));
}

// What: Request a machine copy and click Make Copy in the full app.
// Why: Both sources must reach the save adapter and report outcomes to the user.
#[test]
fn screen_copy_sources_reach_save_and_show_results() {
    // Arrange: Use a fake save adapter so no native dialog opens during the test.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let mut clock = || now;
    let mut snapshots = Vec::new();
    let mut save = |pixels: &[u8]| {
        snapshots.push(pixels.to_vec());
        match snapshots.len() {
            1 => Ok(Some(PathBuf::from("first.png"))),
            2 => Err(io::Error::other("save failed").to_string()),
            _ => Ok(None),
        }
    };
    app.machine.write_byte(0x8795, 0x04).unwrap();
    app.machine.write_byte(0x8794, 0x20).unwrap();
    app.machine.write_byte(0x8794, 0).unwrap();
    let input = |events| RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
        events,
        ..Default::default()
    };

    // Act: Process the machine request, then click the painted Make Copy button.
    let _ = context.run(input(vec![]), |context| {
        app.update_at_with_save(context, now, &mut save, &mut clock);
    });
    let output = context.run(input(vec![]), |context| {
        app.update_at_with_save(context, now, &mut save, &mut clock);
    });
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::Text(text) if text.galley.job.text == "MAKE COPY" => {
                Some(text.pos + Vec2::splat(2.0))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(app.screen_copy_notices[0].1, "Screen copy saved: first.png");
    for pressed in [true, false] {
        let _ = context.run(
            input(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]),
            |context| app.update_at_with_save(context, now, &mut save, &mut clock),
        );
    }

    // Assert: The failed save is visible.
    assert_eq!(
        app.screen_copy_notices,
        [(true, "Screen copy failed: save failed".to_owned())]
    );

    // Act: Click Make Copy again and cancel the save dialog.
    for pressed in [true, false] {
        let _ = context.run(
            input(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]),
            |context| app.update_at_with_save(context, now, &mut save, &mut clock),
        );
    }

    // Assert: Cancellation is reported and every request delivered the same full image.
    assert_eq!(
        app.screen_copy_notices,
        [(false, "Screen copy cancelled".to_owned())]
    );
    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        snapshots[0].len(),
        DisplayModel::WIDTH * DisplayModel::HEIGHT
    );
    assert_eq!(snapshots[0], snapshots[1]);
    assert_eq!(snapshots[1], snapshots[2]);
}

// What: Save a copy triggered between two display updates in the running app.
// Why: The exported PNG must preserve the machine's event-time pixel coordinates and intensity.
#[test]
fn machine_copy_saves_event_time_display_pixels() {
    // Arrange: Draw a stored vector and trigger a copy before extending it.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    for (port, control, value) in [
        (0x878c, 0x878d, 0),
        (0x878e, 0x878f, 20),
        (0x8794, 0x8795, 0x80),
        (0x8796, 0x8797, 5),
    ] {
        app.machine.write_byte(port, 0xff).unwrap();
        app.machine.write_byte(control, 0x04).unwrap();
        app.machine.write_byte(port, value).unwrap();
    }
    app.machine.write_byte(0x8796, 10).unwrap();
    app.machine.write_byte(0x8794, 0xa0).unwrap();
    app.machine.write_byte(0x8794, 0x80).unwrap();
    app.machine.write_byte(0x8796, 20).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("screen.png");
    let mut save = |pixels: &[u8]| {
        save_screen(&path, pixels).map_err(|error| error.to_string())?;
        Ok(Some(path.clone()))
    };
    let now = app.last_update;
    let mut clock = || now;

    // Act: Process the pending machine copy through the app and decode its PNG.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_save(context, now, &mut save, &mut clock);
    });
    let image = image::open(&path).unwrap().to_rgba8();

    // Assert: The earlier asymmetric vector is present, but the later extension is absent.
    assert_eq!(image.get_pixel(26, 799).0, [0, 240, 0, 255]);
    assert_eq!(image.get_pixel(36, 799).0, [0, 0, 0, 255]);
    assert_eq!(app.machine.display().pixel_intensity(36, 799), Some(240));
}

// What: Advance the app after a simulated five-second save dialog.
// Why: Dialog time must not be added to the emulated CPU interval.
#[test]
fn screen_copy_dialog_time_does_not_advance_the_machine() {
    // Arrange: Use NOPs to observe one 10 ms instruction budget.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for offset in 0..5_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    app.machine.write_byte(0x8795, 0x04).unwrap();
    app.machine.write_byte(0x8794, 0x20).unwrap();
    app.machine.write_byte(0x8794, 0).unwrap();
    let start = app.last_update;
    let after_dialog = start + Duration::from_secs(5);
    let mut clock = || after_dialog;
    let mut save = |_: &[u8]| Ok(None);

    // Act: Process the copy, then advance 10 ms after the dialog closes.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_save(context, start, &mut save, &mut clock);
    });
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_save(
            context,
            after_dialog + Duration::from_millis(10),
            &mut save,
            &mut clock,
        );
    });

    // Assert: The CPU ran only the single x1 budget, not five seconds of callbacks.
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264);
    assert!(app.runtime_error.is_none());
}

// What: Process more machine copy requests than the app can buffer.
// Why: Lost requests must be visible to the user without halting emulation.
#[test]
fn screen_copy_overflow_is_reported_without_stopping_runtime() {
    // Arrange: Queue eight snapshots and overflow twice.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    app.machine.write_byte(0x8795, 0x04).unwrap();
    for _ in 0..10 {
        app.machine.write_byte(0x8794, 0x20).unwrap();
        app.machine.write_byte(0x8794, 0).unwrap();
    }
    let now = app.last_update;
    let mut clock = || now;
    let mut saves = 0;
    let mut save = |_: &[u8]| {
        saves += 1;
        Ok(None)
    };

    // Act: Process pending requests and advance the next emulation interval.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_save(context, now, &mut save, &mut clock);
    });
    let frame = context.run(RawInput::default(), |context| {
        app.update_at_with_save(
            context,
            now + Duration::from_millis(10),
            &mut save,
            &mut clock,
        );
    });

    // Assert: Both losses are reported, buffered copies are handled, and the runtime is healthy.
    assert_eq!(saves, 8);
    assert!(
        app.screen_copy_notices
            .iter()
            .any(|(error, message)| *error && message.contains("At least 2"))
    );
    assert!(frame.shapes.iter().any(|shape| matches!(
        &shape.shape,
        Shape::Text(text)
            if text.galley.job.text == "At least 2 screen copy request(s) could not be saved: queue full"
    )));
    assert!(app.runtime_error.is_none());
}
