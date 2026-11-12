use std::time::Duration;

use super::super::front_panel::FrontPanel;
use super::{KeyboardInput, keyboard_key};
use eframe::egui::{self, Key};
use tek4051_emulator::emulation::{keyboard::KeyboardKey, runtime::HostRuntime};
use tek4051_emulator::emulation_input::{InputAction, InputSequence, KeyboardState, ManualKeyId};
use tek4051_emulator::machine::Machine;

#[derive(Default)]
struct KeyboardHarness {
    adapter: KeyboardInput,
    keyboard: KeyboardState,
    sequence: InputSequence,
}

impl KeyboardHarness {
    fn handle_events(
        &mut self,
        machine: &mut Machine,
        events: &[egui::Event],
        modifiers: egui::Modifiers,
        focused: bool,
        panel: Option<&FrontPanel>,
    ) {
        self.adapter
            .translate_events(events, modifiers, focused, panel)
            .apply(&mut self.sequence, &mut self.keyboard, machine);
    }

    fn advance_sequence(&mut self, machine: &mut Machine, elapsed: Duration) {
        let mut runtime = HostRuntime::start(machine);
        runtime.pause();
        self.sequence
            .advance(
                elapsed,
                &mut self.keyboard,
                machine,
                &mut runtime,
                tek4051_emulator::emulation_input::KeyModifiers::NONE,
            )
            .unwrap();
    }
}

fn configure_keyboard_ports(machine: &mut Machine) {
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
}

fn key_event(key: Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

// What: Ignore a repeated host keydown instead of emitting a modifier-only action.
// Why: A held key must not turn every key-repeat event into another worker command.
#[test]
fn repeated_keydown_does_not_create_input_actions() {
    // Arrange: Build one repeat event with unchanged modifiers and a focused input context.
    let event = egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers: egui::Modifiers::NONE,
    };
    let input = KeyboardInput::default();

    // Act: Translate the repeat without applying it to the emulated keyboard.
    let frame = input.translate_events(&[event], egui::Modifiers::NONE, true, None);

    // Assert: The frame has no action that could occupy the worker command queue.
    assert!(frame.actions.is_empty());
}

// What: Translate every supported host key into the code used by the web keyboard.
// Why: A mapping regression would prevent the ROM from seeing typed or panel input.
#[test]
fn egui_keys_resolve_to_framework_neutral_keys() {
    // Arrange: Enumerate the letter/digit keys and every supported alias and panel key.
    let alphanumeric_keys = [
        (Key::A, b'A'),
        (Key::B, b'B'),
        (Key::C, b'C'),
        (Key::D, b'D'),
        (Key::E, b'E'),
        (Key::F, b'F'),
        (Key::G, b'G'),
        (Key::H, b'H'),
        (Key::I, b'I'),
        (Key::J, b'J'),
        (Key::K, b'K'),
        (Key::L, b'L'),
        (Key::M, b'M'),
        (Key::N, b'N'),
        (Key::O, b'O'),
        (Key::P, b'P'),
        (Key::Q, b'Q'),
        (Key::R, b'R'),
        (Key::S, b'S'),
        (Key::T, b'T'),
        (Key::U, b'U'),
        (Key::V, b'V'),
        (Key::W, b'W'),
        (Key::X, b'X'),
        (Key::Y, b'Y'),
        (Key::Z, b'Z'),
        (Key::Num0, b'0'),
        (Key::Num1, b'1'),
        (Key::Num2, b'2'),
        (Key::Num3, b'3'),
        (Key::Num4, b'4'),
        (Key::Num5, b'5'),
        (Key::Num6, b'6'),
        (Key::Num7, b'7'),
        (Key::Num8, b'8'),
        (Key::Num9, b'9'),
    ];
    let special_character_keys = [
        (Key::Space, ' '),
        (Key::Colon, ':'),
        (Key::Quote, '\''),
        (Key::Equals, '='),
        (Key::Comma, ','),
        (Key::Backslash, '\\'),
        (Key::Pipe, '|'),
        (Key::Slash, '/'),
        (Key::Questionmark, '?'),
        (Key::Exclamationmark, '!'),
        (Key::OpenBracket, '['),
        (Key::OpenCurlyBracket, '{'),
        (Key::CloseBracket, ']'),
        (Key::CloseCurlyBracket, '}'),
        (Key::Backtick, '`'),
        (Key::Minus, '-'),
        (Key::Period, '.'),
        (Key::Plus, '+'),
        (Key::Semicolon, ';'),
    ];
    let panel_keys = [
        (Key::Backspace, KeyboardKey::Backspace),
        (Key::Tab, KeyboardKey::Tab),
        (Key::Enter, KeyboardKey::Enter),
        (Key::Escape, KeyboardKey::Escape),
        (Key::Delete, KeyboardKey::Delete),
        (Key::Insert, KeyboardKey::Insert),
        (Key::Home, KeyboardKey::Home),
        (Key::F13, KeyboardKey::Home),
        (Key::End, KeyboardKey::End),
        (Key::F14, KeyboardKey::End),
        (Key::PageUp, KeyboardKey::PageUp),
        (Key::F16, KeyboardKey::PageUp),
        (Key::F17, KeyboardKey::Function(17)),
        (Key::F1, KeyboardKey::Function(1)),
        (Key::F2, KeyboardKey::Function(2)),
        (Key::F3, KeyboardKey::Function(3)),
        (Key::F4, KeyboardKey::Function(4)),
        (Key::F5, KeyboardKey::Function(5)),
        (Key::F6, KeyboardKey::Function(6)),
        (Key::F7, KeyboardKey::Function(7)),
        (Key::F8, KeyboardKey::Function(8)),
        (Key::F9, KeyboardKey::Function(9)),
        (Key::F10, KeyboardKey::Function(10)),
        (Key::F15, KeyboardKey::Function(15)),
    ];
    let unsupported_keys = [
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::ArrowUp,
        Key::Copy,
        Key::Cut,
        Key::F11,
        Key::F12,
        Key::F18,
        Key::F19,
        Key::F20,
        Key::F21,
        Key::F22,
        Key::F23,
        Key::F24,
        Key::F25,
        Key::F26,
        Key::F27,
        Key::F28,
        Key::F29,
        Key::F30,
        Key::F31,
        Key::F32,
        Key::F33,
        Key::F34,
        Key::F35,
        Key::BrowserBack,
        Key::PageDown,
        Key::Paste,
    ];

    // Act: Resolve each key group through the production host-key mapping.
    let actual_alphanumeric: Vec<_> = alphanumeric_keys
        .iter()
        .map(|(key, _)| keyboard_key(*key))
        .collect();
    let actual_special: Vec<_> = special_character_keys
        .iter()
        .map(|(key, _)| (*key, keyboard_key(*key)))
        .collect();
    let actual_panel: Vec<_> = panel_keys
        .iter()
        .map(|(key, _)| (*key, keyboard_key(*key)))
        .collect();
    let actual_unsupported: Vec<_> = unsupported_keys
        .iter()
        .map(|key| keyboard_key(*key))
        .collect();

    // Assert: All supported keys match their source codes and unsupported keys stay unmapped.
    let expected_alphanumeric: Vec<_> = alphanumeric_keys
        .iter()
        .map(|(_, character)| Some(KeyboardKey::Printable(*character as char)))
        .collect();
    let expected_special: Vec<_> = special_character_keys
        .iter()
        .map(|(key, character)| (*key, Some(KeyboardKey::Printable(*character))))
        .collect();
    let expected_panel: Vec<_> = panel_keys
        .iter()
        .map(|(key, host_key)| (*key, Some(*host_key)))
        .collect();
    assert_eq!(actual_alphanumeric, expected_alphanumeric);
    assert_eq!(
        actual_special, expected_special,
        "special character aliases must preserve their Tek key and Shift behavior"
    );
    assert_eq!(actual_panel, expected_panel);
    assert_eq!(actual_unsupported, vec![None; unsupported_keys.len()]);
}

// What: A host key press and release update the machine's keyboard PIA.
// Why: BASIC input is scanned by the emulated ROM, not consumed by the egui layer.
#[test]
fn key_events_press_and_release_the_machine_keyboard() {
    // Arrange: Expose the keyboard PIA data registers for observing host input.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardHarness::default();
    let control = egui::Modifiers {
        ctrl: true,
        ..egui::Modifiers::NONE
    };
    let released = egui::Modifiers::NONE;

    // Act: Forward an A keydown and then its matching keyup.
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::A, true, control)],
        control,
        true,
        None,
    );
    let pressed = machine.read_byte(0x87a8).unwrap();
    let control_port = machine.read_byte(0x87aa).unwrap();
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::A, false, released)],
        released,
        true,
        None,
    );
    keyboard_input.advance_sequence(&mut machine, Duration::from_millis(100));

    // Assert: The PIA exposes A and Control while held, then releases after the minimum press.
    assert_eq!(pressed, 0xc1);
    assert_eq!(control_port, 0x09);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Preserve a complete host key tap delivered in one native frame.
// Why: Applying its down and up edges back-to-back would release the key before a machine tick.
#[test]
fn same_frame_host_tap_remains_held_until_the_minimum_press_elapses() {
    // Arrange: Configure the keyboard PIA and send both edges with their paired text event.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardHarness::default();
    let modifiers = egui::Modifiers::NONE;
    let events = [
        key_event(Key::A, true, modifiers),
        egui::Event::Text("A".to_owned()),
        key_event(Key::A, false, modifiers),
    ];

    // Act: Apply the complete tap, then advance through its minimum held interval.
    keyboard_input.handle_events(&mut machine, &events, modifiers, true, None);
    let held = machine.read_byte(0x87a8).unwrap();
    keyboard_input.advance_sequence(&mut machine, Duration::from_millis(49));
    let held_before_minimum = machine.read_byte(0x87a8).unwrap();
    keyboard_input.advance_sequence(&mut machine, Duration::from_millis(1));

    // Assert: The key survives the same-frame keyup through the 50 ms hold interval.
    assert_eq!(held, b'A' | 0x80);
    assert_eq!(held_before_minimum, b'A' | 0x80);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: A panel key event reaches the machine keyboard PIA.
// Why: Panel mappings must be dispatched, not just translated by the lookup table.
#[test]
fn panel_key_events_reach_the_machine_keyboard() {
    // Arrange: Configure the keyboard PIA and create a fresh host-input adapter.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardHarness::default();
    let modifiers = egui::Modifiers::NONE;

    // Act: Forward a Backspace press and its release through the event handler.
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::Backspace, true, modifiers)],
        modifiers,
        true,
        None,
    );
    let pressed = machine.read_byte(0x87a8).unwrap();
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::Backspace, false, modifiers)],
        modifiers,
        true,
        None,
    );
    keyboard_input.advance_sequence(&mut machine, Duration::from_millis(100));

    // Assert: The panel code is visible while held, then returns to the released value.
    assert_eq!(pressed, 0x88);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Empty and multi-character text events do not produce a Tek key press.
// Why: Only a single ASCII character can safely pair with a host key event.
#[test]
fn invalid_text_events_do_not_press_a_key() {
    // Arrange: Exercise both empty input and a text event containing multiple characters.
    let invalid_text_inputs = ["", "ab"];

    // Act: Pair each invalid text event with an otherwise mappable A keydown.
    let observed_keyboard_values: Vec<_> = invalid_text_inputs
        .iter()
        .map(|text| {
            let mut machine = Machine::default();
            configure_keyboard_ports(&mut machine);
            machine.keyboard_release();
            let mut keyboard_input = KeyboardHarness::default();
            let modifiers = egui::Modifiers::NONE;
            let events = [
                key_event(Key::A, true, modifiers),
                egui::Event::Text((*text).to_owned()),
            ];
            keyboard_input.handle_events(&mut machine, &events, modifiers, true, None);
            machine.read_byte(0x87a8).unwrap()
        })
        .collect();

    // Assert: Neither malformed text event changes the released-key value.
    assert_eq!(observed_keyboard_values, [0x80, 0x80]);
}

// What: An Alt+Control key event paired with text is accepted; unpaired Alt is suppressed.
// Why: The adapter distinguishes printable modified input from untyped Alt shortcuts.
#[test]
fn altgr_text_is_forwarded_but_alt_shortcuts_are_ignored() {
    // Arrange: Configure observable PIA ports and represent AltGr with Alt and Control.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    machine.keyboard_release();
    let mut keyboard_input = KeyboardHarness::default();
    let altgr = egui::Modifiers {
        alt: true,
        ctrl: true,
        ..egui::Modifiers::NONE
    };
    let alt = egui::Modifiers {
        alt: true,
        ..egui::Modifiers::NONE
    };
    let altgr_events = [
        key_event(Key::Q, true, altgr),
        egui::Event::Text("@".to_owned()),
    ];

    // Act: Forward the synthetic at-sign, release its key, then send Alt+A without text.
    keyboard_input.handle_events(&mut machine, &altgr_events, altgr, true, None);
    let at_sign_key = machine.read_byte(0x87a8).unwrap();
    let at_sign_modifiers = machine.read_byte(0x87aa).unwrap();
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::Q, false, altgr)],
        altgr,
        true,
        None,
    );
    keyboard_input.advance_sequence(&mut machine, Duration::from_millis(100));
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::A, true, alt)],
        alt,
        true,
        None,
    );

    // Assert: The typed glyph uses its actual Shift state, but an untyped Alt shortcut is ignored.
    assert_eq!(at_sign_key, 0xc0);
    assert_eq!(at_sign_modifiers, 0x09);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Text from a later key cannot make an earlier Alt shortcut look like AltGr.
// Why: Only text paired with the same key event may bypass Alt shortcut suppression.
#[test]
fn unrelated_text_does_not_allow_an_alt_shortcut() {
    // Arrange: Prepare an Alt shortcut followed by a normal key with printable text.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardHarness::default();
    let alt = egui::Modifiers {
        alt: true,
        ..egui::Modifiers::NONE
    };
    let no_modifiers = egui::Modifiers::NONE;
    let events = [
        key_event(Key::A, true, alt),
        key_event(Key::B, true, no_modifiers),
        egui::Event::Text("@".to_owned()),
    ];

    // Act: Translate the events, inspect the owning key, then apply the frame to the machine.
    let frame = keyboard_input
        .adapter
        .translate_events(&events, no_modifiers, true, None);
    let presses: Vec<_> = frame
        .actions
        .iter()
        .filter_map(|action| match action {
            InputAction::SequencedManualPress {
                key,
                code,
                invert_shift,
                ..
            } => Some((*key, *code, *invert_shift)),
            _ => None,
        })
        .collect();
    frame.apply(
        &mut keyboard_input.sequence,
        &mut keyboard_input.keyboard,
        &mut machine,
    );

    // Assert: B owns the paired text using its event-time Shift state; Alt+A remains suppressed.
    assert_eq!(presses, [(ManualKeyId(Key::B as u64), b'@', false)]);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xc0);
}

// What: Typed characters preserve the web mapping for shifted glyphs.
// Why: A logical key alone cannot distinguish every shifted character on host layouts.
#[test]
fn text_events_map_shifted_glyphs_before_machine_input() {
    // Arrange: Configure observable PIA ports and a fresh keyboard adapter.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardHarness::default();
    let shifted = egui::Modifiers {
        shift: true,
        ..egui::Modifiers::NONE
    };
    let events = [
        key_event(Key::Num2, true, shifted),
        egui::Event::Text("@".to_owned()),
    ];

    // Act: Forward host input for the shifted at-sign.
    keyboard_input.handle_events(&mut machine, &events, shifted, true, None);

    // Assert: The web lookup's at-sign code and inverted Shift line reach the PIA.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xc0);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);
}

// What: A shifted punctuation key uses the reference's inverted Shift line.
// Why: The web keyboard reverses Shift for selected glyphs before sending their Tek key code.
#[test]
fn special_glyphs_invert_shift_and_focus_loss_releases_input() {
    // Arrange: Configure observable PIA ports and a fresh keyboard adapter.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardHarness::default();
    let shifted = egui::Modifiers {
        shift: true,
        ctrl: true,
        ..egui::Modifiers::NONE
    };
    // Act: Press a shifted colon and capture the resulting PIA state.
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::Colon, true, shifted)],
        shifted,
        true,
        None,
    );
    let pressed_key = machine.read_byte(0x87a8).unwrap();
    let inverted_shift_port = machine.read_byte(0x87aa).unwrap();

    // Act: Simulate the native window losing focus before a keyup is received.
    keyboard_input.handle_events(&mut machine, &[], egui::Modifiers::NONE, false, None);

    // Assert: The glyph uses the inverted Shift line, and focus loss clears Control.
    assert_eq!(pressed_key, 0xba);
    assert_eq!(inverted_shift_port, 0x09);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);
}
