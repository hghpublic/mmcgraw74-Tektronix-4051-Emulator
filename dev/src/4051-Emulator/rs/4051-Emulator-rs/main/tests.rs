use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use tek4051_emulator::{host_runtime::HostRuntime, machine::Machine};

use super::{Tek4051App, speed_control};

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
