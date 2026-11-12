use std::time::Duration;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use tek4051_emulator::{
    emulation_input::{AutoLoadState, InputSequence, KeyboardState},
    host_runtime::HostRuntime,
    machine::{Machine, cpu},
};

use super::super::keyboard_input::{KeyboardInput, key_modifiers};
use super::FrontPanel;

struct PanelHarness {
    context: egui::Context,
    panel: FrontPanel,
    sequence: InputSequence,
    keyboard: KeyboardInput,
    state: KeyboardState,
    machine: Machine,
    runtime: HostRuntime,
    last_modifiers: Modifiers,
    clip_rect: Option<Rect>,
}

impl PanelHarness {
    fn new() -> Self {
        let mut machine = Machine::default();
        machine.write_byte(0x87a9, 0x04).unwrap();
        machine.write_byte(0x87ab, 0x04).unwrap();
        machine.keyboard_release();
        machine.write_byte(0x0100, 0x20).unwrap();
        machine.write_byte(0x0101, 0xfe).unwrap();
        machine.cpu_mut().registers.pc = 0x0100;
        machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
        let runtime = HostRuntime::start(&mut machine);
        Self {
            context: egui::Context::default(),
            panel: FrontPanel::default(),
            sequence: InputSequence::default(),
            keyboard: KeyboardInput::default(),
            state: KeyboardState::default(),
            machine,
            runtime,
            last_modifiers: Modifiers::NONE,
            clip_rect: None,
        }
    }

    fn frame(
        &mut self,
        events: Vec<Event>,
        modifiers: Modifiers,
        focused: bool,
        elapsed: Duration,
    ) -> Vec<(String, Pos2)> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 200.0))),
            events,
            modifiers,
            focused,
            ..Default::default()
        };
        let output = self.context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                if let Some(clip_rect) = self.clip_rect {
                    ui.set_clip_rect(ui.clip_rect().intersect(clip_rect));
                }
                self.panel.show(ui);
            });
            self.sequence
                .advance(
                    elapsed,
                    &mut self.state,
                    &mut self.machine,
                    &mut self.runtime,
                    key_modifiers(self.last_modifiers),
                )
                .unwrap();
            let (events, modifiers, focused) =
                context.input(|input| (input.raw.events.clone(), input.modifiers, input.focused));
            let frame = self.keyboard.translate_frame(
                &self.panel,
                (&events, modifiers, focused),
                false,
                &[],
            );
            frame.apply(&mut self.sequence, &mut self.state, &mut self.machine);
            self.last_modifiers = if focused { modifiers } else { Modifiers::NONE };
        });
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Text(text) => {
                    Some((text.galley.job.text.clone(), text.pos + Vec2::splat(2.0)))
                }
                _ => None,
            })
            .collect()
    }

    fn key_value(&mut self) -> u8 {
        self.machine.read_byte(0x87a8).unwrap()
    }
}

fn pointer(position: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn host_key(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

// What: Deliver Auto Load's complete file-1 sequence with observable press/release intervals.
// Why: The ROM must receive ordered keys without inherited Shift or duplicate queued commands.
#[test]
fn auto_load_sends_fixed_commands_with_distinct_unmodified_taps() {
    // Arrange: Use a running board fixture while the host Shift modifier remains held.
    let mut harness = PanelHarness::new();
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    harness.frame(vec![], shift, true, Duration::ZERO);

    // Act: Start the macro and reject a second request while its keyboard input is pending.
    harness
        .sequence
        .queue_auto_load(&mut harness.state, &mut harness.machine)
        .unwrap();
    assert_eq!(
        harness
            .sequence
            .queue_auto_load(&mut harness.state, &mut harness.machine),
        Err("keyboard input is busy")
    );

    // Assert: Script keys remain held for 50 ms, then stay released for 30 ms before the next key.
    for &code in b"FIND@5:1\rOLD@5:\rRUN\r" {
        assert_eq!(harness.key_value(), code | 0x80);
        assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
        harness.frame(vec![], shift, true, Duration::from_millis(49));
        assert_eq!(harness.key_value(), code | 0x80);
        harness.frame(vec![], shift, true, Duration::from_millis(1));
        assert_eq!(harness.key_value(), 0x80);
        harness.frame(vec![], shift, true, Duration::from_millis(29));
        assert_eq!(harness.key_value(), 0x80);
        harness.frame(vec![], shift, true, Duration::from_millis(1));
    }
    assert_eq!(harness.sequence.auto_load_state(), AutoLoadState::Sent);
    assert_eq!(harness.key_value(), 0x80);
}

fn advance_script_command(harness: &mut PanelHarness, command: &[u8]) {
    for &character in command {
        match character {
            b'"' => {
                assert_eq!(harness.key_value(), b'2' | 0x80);
                assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 0);
            }
            b',' => {
                assert_eq!(harness.key_value(), b'<' | 0x80);
                assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
            }
            _ => assert_ne!(harness.key_value(), 0x80),
        }
        harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(50));
        assert_eq!(harness.key_value(), 0x80);
        harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(30));
    }
}

// What: Queue a selected program's FIND/OLD/RUN commands one BASIC prompt at a time.
// Why: A command can take longer than the key-tap interval; the next line must await ROM readiness.
#[test]
fn selected_program_load_waits_for_basic_prompt_between_commands() {
    // Arrange: Start a fresh panel harness for a multi-digit Flash Drive file number.
    let mut harness = PanelHarness::new();

    // Act: Queue file 24 and type only its FIND command.
    harness
        .sequence
        .queue_program_load(24, &mut harness.state, &mut harness.machine)
        .unwrap();
    advance_script_command(&mut harness, b"FIND@5:24\r");

    // Assert: OLD stays queued until BASIC reports that FIND has returned to its prompt.
    assert_eq!(harness.sequence.auto_load_state(), AutoLoadState::Typing);
    assert!(!harness.sequence.resume_program_load_if_ready(
        false,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), 0x80);
    assert!(harness.sequence.resume_program_load_if_ready(
        true,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), b'O' | 0x80);

    // Act: Type OLD and then release RUN only after another ready signal.
    advance_script_command(&mut harness, b"OLD@5:\r");
    assert!(!harness.sequence.resume_program_load_if_ready(
        false,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), 0x80);
    assert!(harness.sequence.resume_program_load_if_ready(
        true,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), b'R' | 0x80);
    advance_script_command(&mut harness, b"RUN\r");

    // Assert: The selected program sequence completes only after its final key is released.
    assert_eq!(harness.sequence.auto_load_state(), AutoLoadState::Sent);
    assert_eq!(harness.key_value(), 0x80);
}

// What: Queue binary FIND/BOLD/RUN commands and encode BOLD's punctuation as Tek keys.
// Why: BOLD is a long transfer; its command and RUN must be separate, correctly typed BASIC lines.
#[test]
fn selected_binary_program_load_waits_for_prompts_and_maps_bold_symbols() {
    // Arrange: Start a fresh panel harness for the supplied binary-launch command sequence.
    let mut harness = PanelHarness::new();

    // Act: Queue binary file 24 and deliver FIND without sending the BOLD call early.
    harness
        .sequence
        .queue_binary_program_load(24, &mut harness.state, &mut harness.machine)
        .unwrap();
    advance_script_command(&mut harness, b"FIND@5:24\r");

    // Assert: The next line is withheld until a fresh BASIC-ready signal is supplied.
    assert!(!harness.sequence.resume_program_load_if_ready(
        false,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), 0x80);
    assert!(harness.sequence.resume_program_load_if_ready(
        true,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), b'C' | 0x80);

    // Act: Type CALL with the mapped quote and comma, then wait again before RUN.
    advance_script_command(&mut harness, b"CALL \"BOLD\",5\r");
    assert!(!harness.sequence.resume_program_load_if_ready(
        false,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), 0x80);
    assert!(harness.sequence.resume_program_load_if_ready(
        true,
        &mut harness.state,
        &mut harness.machine,
    ));
    assert_eq!(harness.key_value(), b'R' | 0x80);
    advance_script_command(&mut harness, b"RUN\r");

    // Assert: RUN is the last command and the sequence reports completion after its release.
    assert_eq!(harness.sequence.auto_load_state(), AutoLoadState::Sent);
    assert_eq!(harness.key_value(), 0x80);
}

// What: Interrupt Auto Load by focus loss, a host key, or a manual panel press.
// Why: Unsent macro keys must not resume later or overwrite the user's replacement input.
#[test]
fn auto_load_cancels_on_focus_loss_and_manual_input() {
    for interruption in ["focus", "host", "panel"] {
        // Arrange: Start a fresh macro after obtaining the manual user-key position.
        let mut harness = PanelHarness::new();
        let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
        let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
        harness
            .sequence
            .queue_auto_load(&mut harness.state, &mut harness.machine)
            .unwrap();

        // Act: Replace or release the macro's active key through the normal input adapter.
        let events = match interruption {
            "host" => vec![host_key(Key::Z, true)],
            "panel" => vec![pointer(position, true, Modifiers::NONE)],
            _ => vec![],
        };
        harness.frame(
            events,
            Modifiers::NONE,
            interruption != "focus",
            Duration::ZERO,
        );

        // Assert: Cancellation is distinct from completion and replacement input owns the keyboard.
        assert_eq!(harness.sequence.auto_load_state(), AutoLoadState::Cancelled);
        assert_eq!(
            harness.key_value(),
            match interruption {
                "host" => 0xda,
                "panel" => 0xe0,
                _ => 0x80,
            }
        );
        harness.frame(vec![], Modifiers::NONE, true, Duration::from_secs(5));
        assert_eq!(harness.sequence.auto_load_state(), AutoLoadState::Cancelled);
        assert_eq!(
            harness.key_value(),
            match interruption {
                "host" => 0xda,
                "panel" => 0xe0,
                _ => 0x80,
            }
        );
    }
}

// What: Press and release every user key through the rendered egui buttons.
// Why: The ten visible controls must send the web reference's codes and respect Shift.
#[test]
fn user_buttons_send_reference_keys_and_shift() {
    // Arrange: Paint the panel and locate each button by its visible label.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Hold each button both with and without Shift, then release it.
    for (modifiers, shift_bit) in [(Modifiers::NONE, 1), (shift, 0)] {
        for index in 0..10_u8 {
            let label = format!("{} / {}", index + 1, index + 11);
            let position = labels.iter().find(|(text, _)| text == &label).unwrap().1;
            harness.frame(
                vec![
                    Event::PointerMoved(position),
                    pointer(position, true, modifiers),
                ],
                modifiers,
                true,
                Duration::from_millis(10),
            );

            // Assert: Both functions use the same code and the selected Shift line.
            assert_eq!(harness.key_value(), 0xe0 + index);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, shift_bit);
            harness.frame(
                vec![pointer(position, false, modifiers)],
                Modifiers::NONE,
                true,
                Duration::from_millis(100),
            );
            assert_eq!(harness.key_value(), 0x80);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
            harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
        }
    }
}

// What: Press each line-editor button with and without Shift.
// Why: Both labeled functions must use the reference key code and modifier line.
#[test]
fn line_editor_buttons_send_reference_keys_and_shift() {
    // Arrange: Paint the panel and locate the five line-editor buttons by label.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let editor_keys = [
        ("EXPAND / COMPRESS", 0x70),
        ("BACK SPACE / RUB OUT LEFT", 0x71),
        ("SPACE / RUB OUT RIGHT", 0x72),
        ("CLEAR / REPRINT", 0x73),
        ("RECALL LINE / RECALL NEXT LINE", 0x74),
    ];
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Hold and release every button in both modifier states.
    for (modifiers, shift_bit) in [(Modifiers::NONE, 1), (shift, 0)] {
        for (label, code) in editor_keys {
            let position = labels.iter().find(|(text, _)| text == label).unwrap().1;
            harness.frame(
                vec![
                    Event::PointerMoved(position),
                    pointer(position, true, modifiers),
                ],
                modifiers,
                true,
                Duration::ZERO,
            );

            // Assert: The PIA exposes the selected code and active-low Shift state.
            assert_eq!(harness.key_value(), 0x80 | code);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, shift_bit);
            harness.frame(
                vec![pointer(position, false, modifiers)],
                Modifiers::NONE,
                true,
                Duration::from_millis(100),
            );
            assert_eq!(harness.key_value(), 0x80);
            harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
        }
    }
}

// What: Press the Auto Number and Step Program buttons in both Shift states.
// Why: Program controls must send the reference codes without changing the modifier line.
#[test]
fn program_buttons_send_reference_keys_and_shift() {
    // Arrange: Paint the panel and locate both program controls.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Hold and release each program button with and without Shift.
    for (modifiers, shift_bit) in [(Modifiers::NONE, 1), (shift, 0)] {
        for (label, code) in [("AUTO NUMBER", 0x75), ("STEP PROGRAM", 0x76)] {
            let position = labels.iter().find(|(text, _)| text == label).unwrap().1;
            harness.frame(
                vec![
                    Event::PointerMoved(position),
                    pointer(position, true, modifiers),
                ],
                modifiers,
                true,
                Duration::ZERO,
            );

            // Assert: The key code and active-low modifier reach the keyboard PIA.
            assert_eq!(harness.key_value(), 0x80 | code);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, shift_bit);
            harness.frame(
                vec![pointer(position, false, modifiers)],
                Modifiers::NONE,
                true,
                Duration::from_millis(100),
            );
            assert_eq!(harness.key_value(), 0x80);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
            harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
        }
    }
}

// What: Press the Home/Page, Rub Out, and Break buttons in both Shift states.
// Why: Special keyboard buttons must deliver the web codes and current modifier.
#[test]
fn special_buttons_send_reference_keys_and_shift() {
    // Arrange: Paint the panel and locate the three special keyboard buttons.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Hold and release each button with and without Shift.
    for (modifiers, shift_bit) in [(Modifiers::NONE, 1), (shift, 0)] {
        for (label, code) in [("HOME / PAGE", 0x6a), ("RUB OUT", 0x5f), ("BREAK", 0x6b)] {
            let position = labels.iter().find(|(text, _)| text == label).unwrap().1;
            harness.frame(
                vec![
                    Event::PointerMoved(position),
                    pointer(position, true, modifiers),
                ],
                modifiers,
                true,
                Duration::ZERO,
            );

            // Assert: The PIA sees the selected code and active-low Shift state.
            assert_eq!(harness.key_value(), 0x80 | code);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, shift_bit);
            harness.frame(
                vec![pointer(position, false, modifiers)],
                Modifiers::NONE,
                true,
                Duration::from_millis(100),
            );
            assert_eq!(harness.key_value(), 0x80);
            assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
            harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
        }
    }
}

// What: Queue a user key followed by a shifted line-editor key.
// Why: Different front-panel groups must share ordered press and release timing.
#[test]
fn user_and_editor_taps_share_the_input_queue() {
    // Arrange: Locate a user key and the line-editor Clear/Reprint button.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let user = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    let editor = labels
        .iter()
        .find(|(text, _)| text == "CLEAR / REPRINT")
        .unwrap()
        .1;
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Deliver both clicks before the first key's hold time expires.
    harness.frame(
        vec![
            Event::PointerMoved(user),
            pointer(user, true, Modifiers::NONE),
            pointer(user, false, Modifiers::NONE),
            Event::PointerMoved(editor),
            pointer(editor, true, shift),
            pointer(editor, false, shift),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Assert: The keys and their modifiers arrive in order with a released interval.
    assert_eq!(harness.key_value(), 0xe0);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(50));
    assert_eq!(harness.key_value(), 0x80);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(29));
    assert_eq!(harness.key_value(), 0x80);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(1));
    assert_eq!(harness.key_value(), 0xf3);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(49));
    assert_eq!(harness.key_value(), 0xf3);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(1));
    assert_eq!(harness.key_value(), 0x80);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
}

// What: Click both visible and clipped portions of a line-editor button.
// Why: A pointer outside the panel's clip must not activate a hidden control.
#[test]
fn line_editor_button_only_accepts_visible_pointer_input() {
    // Arrange: Paint the editor, then clip halfway across the Clear/Reprint button.
    let mut harness = PanelHarness::new();
    harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let rect = harness.panel.key_rects[super::USER_KEY_COUNT + 3].0;
    let visible = Pos2::new(rect.left() + rect.width() * 0.25, rect.center().y);
    let hidden = Pos2::new(rect.left() + rect.width() * 0.75, rect.center().y);
    harness.clip_rect = Some(Rect::from_min_max(
        Pos2::ZERO,
        Pos2::new(rect.center().x, 200.0),
    ));
    harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);

    // Act: Click the clipped portion, then the still-visible portion.
    harness.frame(
        vec![
            Event::PointerMoved(hidden),
            pointer(hidden, true, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    // Assert: Only the visible portion delivers the Clear key to the PIA.
    assert_eq!(harness.key_value(), 0x80);
    harness.frame(
        vec![
            pointer(hidden, false, Modifiers::NONE),
            Event::PointerMoved(visible),
            pointer(visible, true, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xf3);
}

// What: Preserve a complete pointer tap delivered in one egui frame.
// Why: The emulated keyboard and CPU need time to process a short tap.
#[test]
fn same_frame_tap_is_held_for_the_observation_window() {
    // Arrange: Locate the first user key on a focused panel.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;

    // Act: Deliver pointer down and up together, then advance across the hold boundary.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(49));
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(1));

    // Assert: The key is released only after the full 50 ms hold interval.
    assert_eq!(harness.key_value(), 0x80);
}

// What: Deliver a short user-key tap to the running base ROM.
// Why: A transient shifted egui click must be observed by the real keyboard handler.
#[test]
fn short_tap_reaches_the_running_rom() {
    // Arrange: Start the normal machine and locate the first user key.
    let mut harness = PanelHarness::new();
    harness.machine.reset().unwrap();
    harness.runtime = HostRuntime::start(&mut harness.machine);
    harness
        .runtime
        .advance(&mut harness.machine, Duration::from_secs(20))
        .unwrap();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Release Shift and the pointer before the 50 ms hold interval expires.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, shift),
        ],
        shift,
        true,
        Duration::ZERO,
    );
    harness.frame(
        vec![pointer(position, false, shift)],
        Modifiers::NONE,
        true,
        Duration::from_millis(1),
    );
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(49));

    // Assert: The base ROM records the pressed key before the minimum hold expires.
    assert_eq!(harness.machine.read_byte(0x059c).unwrap(), 0x60);
    assert_eq!(harness.machine.read_byte(0x0597).unwrap(), 1);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
    harness
        .runtime
        .advance(&mut harness.machine, Duration::from_millis(100))
        .unwrap();
    assert_eq!(harness.machine.read_byte(0x0597).unwrap(), 0);
}

// What: Deliver a complete native host-key tap to the running base ROM in one frame.
// Why: The keyup must not hide a short press before the ROM can scan the keyboard.
#[test]
fn same_frame_host_tap_reaches_the_running_rom() {
    // Arrange: Start the base ROM and prepare a typed "1" keydown/text/keyup triplet.
    let mut harness = PanelHarness::new();
    harness.machine.reset().unwrap();
    harness.runtime = HostRuntime::start(&mut harness.machine);
    harness
        .runtime
        .advance(&mut harness.machine, Duration::from_secs(20))
        .unwrap();
    let modifiers = Modifiers::NONE;

    // Act: Deliver both physical key edges together, then advance through almost all of the hold.
    harness.frame(
        vec![
            host_key(Key::Num1, true),
            Event::Text("1".to_owned()),
            host_key(Key::Num1, false),
        ],
        modifiers,
        true,
        Duration::ZERO,
    );
    harness.frame(vec![], modifiers, true, Duration::from_millis(49));

    // Assert: The base ROM records the key before the minimum manual-key interval completes.
    assert_eq!(harness.machine.read_byte(0x059c).unwrap(), b'1');
    assert_eq!(harness.machine.read_byte(0x0597).unwrap(), 1);
    assert_eq!(harness.key_value(), b'1' | 0x80);
    harness.frame(vec![], modifiers, true, Duration::from_millis(1));
    assert_eq!(harness.key_value(), 0x80);
}

// What: Exercise overlapping physical and panel input and focus loss.
// Why: Releasing an older input source must not release a newer key or leave one latched.
#[test]
fn overlapping_sources_and_focus_loss_do_not_latch_or_release_the_wrong_key() {
    // Arrange: Locate the first panel button and begin a held pointer press.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Act: Press A after the panel key, then release the older pointer press.
    harness.frame(
        vec![host_key(Key::A, true)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    harness.frame(
        vec![pointer(position, false, Modifiers::NONE)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Assert: The newer host key remains active until its own release.
    assert_eq!(harness.key_value(), 0xc1);
    harness.frame(
        vec![host_key(Key::A, false)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
    assert_eq!(harness.key_value(), 0x80);

    // Act: Press a shifted panel key after A, then release A and lose focus.
    harness.frame(
        vec![host_key(Key::A, true)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    harness.frame(
        vec![pointer(position, true, shift)],
        shift,
        true,
        Duration::ZERO,
    );
    harness.frame(
        vec![host_key(Key::A, false)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], shift, true, Duration::ZERO);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 0);
    harness.frame(vec![], Modifiers::NONE, false, Duration::ZERO);

    // Assert: Focus loss clears the current panel input and both modifier lines.
    assert_eq!(harness.key_value(), 0x80);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 3, 3);
}

// What: Preserve distinct transitions for rapid repeated pointer taps.
// Why: Two clicks must not merge into one held user key.
#[test]
fn rapid_taps_have_an_observable_release_between_them() {
    // Arrange: Locate two user keys and prepare a shifted second press.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    let second = labels.iter().find(|(text, _)| text == "2 / 12").unwrap().1;
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Deliver two complete clicks in one frame and advance through their deadlines.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
            Event::PointerMoved(second),
            pointer(second, true, shift),
            pointer(second, false, shift),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xe0);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(50));
    assert_eq!(harness.key_value(), 0x80);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(30));
    assert_eq!(harness.key_value(), 0xe1);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(80));

    // Assert: Both presses were separated by a released interval and finish released.
    assert_eq!(harness.key_value(), 0x80);
}

// What: Handle host and pointer events in their original frame order.
// Why: The most recent input must own the shared machine key.
#[test]
fn same_frame_events_keep_source_order() {
    // Arrange: Locate a user key and prepare two orderings.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;

    // Act: Tap the panel, then press a host key in the same frame.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
            host_key(Key::A, true),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xc1);
    harness.frame(
        vec![host_key(Key::A, false)],
        Modifiers::NONE,
        true,
        Duration::from_millis(300),
    );
    assert_eq!(harness.key_value(), 0x80);

    // Act: Press the host key first, then tap the panel.
    harness.frame(
        vec![
            host_key(Key::A, true),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Assert: The later panel input owns the key; releasing A does not clear it.
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(
        vec![host_key(Key::A, false)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xe0);
}

// What: Release a user key outside its button in the same frame as the press.
// Why: Dragging away must not discard the original press or leave the key stuck.
#[test]
fn same_frame_drag_away_still_sends_and_releases_the_key() {
    // Arrange: Locate the first user key and an outside position.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    let outside = Pos2::new(850.0, 180.0);

    // Act: Press on the button and release outside before the next frame.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            Event::PointerMoved(outside),
            pointer(outside, false, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Assert: The press is observed and subsequently released.
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
    assert_eq!(harness.key_value(), 0x80);
}

// What: Sample the modifier state before and after a pointer-release event.
// Why: New input must not affect prior elapsed time or lose release-event Shift.
#[test]
fn release_event_modifiers_reach_cpu_before_advancing() {
    // Arrange: Install a program that records the keyboard modifier PIA port.
    let mut harness = PanelHarness::new();
    for (offset, byte) in [0xb6, 0x87, 0xaa, 0xb7, 0x02, 0x00, 0x20, 0xfe]
        .into_iter()
        .enumerate()
    {
        harness
            .machine
            .write_byte(0x0100 + offset as u16, byte)
            .unwrap();
    }
    harness.machine.write_byte(0x0200, 0xff).unwrap();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    // Act: Release the button with Shift, while the frame ends with Shift released.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, shift),
        ],
        Modifiers::NONE,
        true,
        Duration::from_millis(10),
    );

    // Assert: The elapsed time before the event used the previous unshifted state.
    assert_eq!(harness.machine.read_byte(0x0200).unwrap(), 0x0b);
    harness.machine.cpu_mut().registers.pc = 0x0100;
    harness.machine.write_byte(0x0200, 0xff).unwrap();
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(10));

    // Assert: The next CPU interval observes Shift during the deferred press.
    assert_eq!(harness.machine.read_byte(0x0200).unwrap(), 0x0a);
}

// What: Release a held pointer key without receiving a pointer-up event.
// Why: Pointer loss must neither latch a key nor discard a short press.
#[test]
fn pointer_gone_preserves_active_and_queued_presses() {
    // Arrange: Start one key and queue a second tap.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
            pointer(position, true, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xe0);

    // Act: Simulate the integration reporting pointer loss instead of a release.
    harness.frame(
        vec![Event::PointerGone],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(50));
    assert_eq!(harness.key_value(), 0x80);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(30));
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(50));
    assert_eq!(harness.key_value(), 0x80);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(30));

    // Act: Press a key and lose the pointer within the same frame.
    harness.frame(
        vec![pointer(position, true, Modifiers::NONE), Event::PointerGone],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Assert: The active key is observed, then released without a pointer-up event.
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(50));
    assert_eq!(harness.key_value(), 0x80);
}

// What: Deliver touch-up followed by pointer loss in the same frame.
// Why: A completed touch tap must not be discarded before the ROM can observe it.
#[test]
fn pointer_gone_after_touch_up_preserves_completed_tap() {
    // Arrange: Locate the first user key.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;

    // Act: Deliver the pointer events emitted for touch-up and disappearance.
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
            Event::PointerGone,
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Assert: The completed tap remains pressed until its observation window ends.
    assert_eq!(harness.key_value(), 0xe0);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(100));
    assert_eq!(harness.key_value(), 0x80);
}

// What: Process a complete host keystroke while a panel tap is queued.
// Why: An accepted newer host press must cancel older panel work even after its keyup.
#[test]
fn complete_host_keystroke_cancels_pending_panel_taps() {
    // Arrange: Queue a second tap behind an active panel press.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let position = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    harness.frame(
        vec![
            Event::PointerMoved(position),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
            pointer(position, true, Modifiers::NONE),
            pointer(position, false, Modifiers::NONE),
        ],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );

    // Act: Press and release A in the same frame and advance beyond both queued taps.
    harness.frame(
        vec![host_key(Key::A, true), host_key(Key::A, false)],
        Modifiers::NONE,
        true,
        Duration::ZERO,
    );
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(500));

    // Assert: The host keyup leaves the keyboard released, without a stale panel press.
    assert_eq!(harness.key_value(), 0x80);
}

// What: Change Shift while a held panel press is waiting behind another tap.
// Why: A queued key still held by the pointer must use current modifiers when it starts.
#[test]
fn queued_held_key_uses_current_modifiers() {
    // Arrange: Queue a shifted second key behind the first key's press and release.
    let mut harness = PanelHarness::new();
    let labels = harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let first = labels.iter().find(|(text, _)| text == "1 / 11").unwrap().1;
    let second = labels.iter().find(|(text, _)| text == "2 / 12").unwrap().1;
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    harness.frame(
        vec![
            Event::PointerMoved(first),
            pointer(first, true, Modifiers::NONE),
            pointer(first, false, Modifiers::NONE),
            Event::PointerMoved(second),
            pointer(second, true, shift),
        ],
        shift,
        true,
        Duration::ZERO,
    );

    // Act: Release Shift while holding the second key, then advance until it starts.
    harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    harness.frame(vec![], Modifiers::NONE, true, Duration::from_millis(200));

    // Assert: The second code is active with Shift released.
    assert_eq!(harness.key_value(), 0xe1);
    assert_eq!(harness.machine.read_byte(0x87aa).unwrap() & 1, 1);
}

// What: Pair AltGr text with its host key across an intervening pointer event.
// Why: Panel dispatch must not discard layout-dependent text or reorder input sources.
#[test]
fn pointer_event_does_not_break_host_key_text_pairing() {
    // Arrange: Represent an AltGr-generated at-sign and an unrelated pointer click.
    let mut harness = PanelHarness::new();
    harness.frame(vec![], Modifiers::NONE, true, Duration::ZERO);
    let altgr = Modifiers {
        alt: true,
        ctrl: true,
        ..Modifiers::NONE
    };
    let outside = Pos2::new(850.0, 180.0);
    let key = Event::Key {
        key: Key::Q,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: altgr,
    };

    // Act: Place a pointer event between the host keydown and its text.
    harness.frame(
        vec![key, pointer(outside, true, altgr), Event::Text("@".into())],
        altgr,
        true,
        Duration::ZERO,
    );

    // Assert: The reference at-sign mapping, not Q or no key, reaches the PIA.
    assert_eq!(harness.key_value(), 0xc0);
}
