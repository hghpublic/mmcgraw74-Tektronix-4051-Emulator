use super::{KeyboardInput, MappedKey, map_host_key, map_text_character};
use eframe::egui::{self, Key};
use tek4051_emulator::machine::Machine;

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

// What: Translate every supported host key into the code used by the web keyboard.
// Why: A mapping regression would prevent the ROM from seeing typed or panel input.
#[test]
fn host_keys_map_to_reference_keyboard_codes() {
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
        (Key::Space, 0x10, false),
        (Key::Colon, b':', true),
        (Key::Quote, b'7', true),
        (Key::Equals, b'=', true),
        (Key::Comma, b'<', false),
        (Key::Backslash, b'\\', false),
        (Key::Pipe, b'\\', false),
        (Key::Slash, b'?', false),
        (Key::Questionmark, b'?', false),
        (Key::Exclamationmark, b'1', false),
        (Key::OpenBracket, b'[', false),
        (Key::OpenCurlyBracket, b'[', false),
        (Key::CloseBracket, b']', false),
        (Key::CloseCurlyBracket, b']', false),
        (Key::Backtick, b'@', false),
        (Key::Minus, b'=', false),
        (Key::Period, b'>', false),
        (Key::Plus, b';', false),
        (Key::Semicolon, b';', false),
    ];
    let panel_keys = [
        (Key::Backspace, 0x08),
        (Key::Tab, 0x09),
        (Key::Enter, 0x0d),
        (Key::Escape, 0x1b),
        (Key::Delete, 0x5f),
        (Key::Insert, 0x73),
        (Key::Home, 0x6a),
        (Key::F13, 0x6a),
        (Key::End, 0x6b),
        (Key::F14, 0x6b),
        (Key::PageUp, 0x75),
        (Key::F16, 0x75),
        (Key::F17, 0x76),
        (Key::F1, 0x60),
        (Key::F2, 0x61),
        (Key::F3, 0x62),
        (Key::F4, 0x63),
        (Key::F5, 0x64),
        (Key::F6, 0x65),
        (Key::F7, 0x66),
        (Key::F8, 0x67),
        (Key::F9, 0x68),
        (Key::F10, 0x69),
        (Key::F15, 0x73),
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
        .map(|(key, _)| map_host_key(*key))
        .collect();
    let actual_special: Vec<_> = special_character_keys
        .iter()
        .map(|(key, _, _)| (*key, map_host_key(*key)))
        .collect();
    let actual_panel: Vec<_> = panel_keys
        .iter()
        .map(|(key, _)| (*key, map_host_key(*key)))
        .collect();
    let actual_unsupported: Vec<_> = unsupported_keys
        .iter()
        .map(|key| map_host_key(*key))
        .collect();

    // Assert: All supported keys match their source codes and unsupported keys stay unmapped.
    let expected_alphanumeric: Vec<_> = alphanumeric_keys
        .iter()
        .map(|(_, code)| Some(MappedKey::character(*code, false)))
        .collect();
    let expected_special: Vec<_> = special_character_keys
        .iter()
        .map(|(key, code, invert_shift)| (*key, Some(MappedKey::character(*code, *invert_shift))))
        .collect();
    let expected_panel: Vec<_> = panel_keys
        .iter()
        .map(|(key, code)| (*key, Some(MappedKey::panel(*code))))
        .collect();
    assert_eq!(actual_alphanumeric, expected_alphanumeric);
    assert_eq!(
        actual_special, expected_special,
        "special character aliases must preserve their Tek key and Shift behavior"
    );
    assert_eq!(actual_panel, expected_panel);
    assert_eq!(actual_unsupported, vec![None; unsupported_keys.len()]);
}

// What: Translate printable text into the web keyboard's ASCII lookup codes.
// Why: Layout-dependent glyphs and unsupported text must not become incorrect PIA input.
#[test]
fn printable_text_matches_reference_character_mapping() {
    // Arrange: Cover punctuation translations, Shift exceptions, and alphabetic case.
    let mappings = [
        (' ', 0x10, false),
        ('!', b'1', false),
        ('"', b'2', false),
        ('#', b'3', true),
        ('$', b'4', false),
        ('%', b'5', false),
        ('&', b'6', false),
        ('\'', b'7', true),
        ('(', b'8', false),
        (')', b'9', false),
        ('*', b'*', false),
        ('+', b';', false),
        (',', b'<', false),
        ('-', b'=', false),
        ('.', b'>', false),
        ('/', b'?', false),
        (':', b':', true),
        (';', b';', false),
        ('<', b'<', false),
        ('=', b'=', true),
        ('>', b'>', false),
        ('?', b'?', false),
        ('@', b'@', true),
        ('[', b'[', false),
        ('\\', b'\\', false),
        (']', b']', false),
        ('^', b'^', false),
        ('_', b'_', false),
        ('`', b'@', false),
        ('{', b'[', false),
        ('|', b'\\', false),
        ('}', b']', false),
        ('~', b'^', true),
        ('0', b'0', false),
        ('9', b'9', false),
        ('A', b'A', false),
        ('a', b'A', false),
        ('z', b'Z', false),
        ('Z', b'Z', false),
    ];

    // Act: Translate each printable character and check non-text input boundaries.
    let actual: Vec<_> = mappings
        .iter()
        .map(|(character, _, _)| map_text_character(*character))
        .collect();
    let expected: Vec<_> = mappings
        .iter()
        .map(|(_, code, invert_shift)| Some(MappedKey::character(*code, *invert_shift)))
        .collect();

    // Assert: Printable glyphs use their reference codes; controls and non-ASCII are ignored.
    assert_eq!(actual, expected);
    assert_eq!(map_text_character('\n'), None);
    assert_eq!(map_text_character('é'), None);
}

// What: A host key press and release update the machine's keyboard PIA.
// Why: BASIC input is scanned by the emulated ROM, not consumed by the egui layer.
#[test]
fn key_events_press_and_release_the_machine_keyboard() {
    // Arrange: Expose the keyboard PIA data registers for observing host input.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardInput::default();
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

    // Assert: The PIA exposes A and Control while held, then the released-key value.
    assert_eq!(pressed, 0xc1);
    assert_eq!(control_port, 0x09);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: A panel key event reaches the machine keyboard PIA.
// Why: Panel mappings must be dispatched, not just translated by the lookup table.
#[test]
fn panel_key_events_reach_the_machine_keyboard() {
    // Arrange: Configure the keyboard PIA and create a fresh host-input adapter.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardInput::default();
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
            let mut keyboard_input = KeyboardInput::default();
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
    let mut keyboard_input = KeyboardInput::default();
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
    keyboard_input.handle_events(
        &mut machine,
        &[key_event(Key::A, true, alt)],
        alt,
        true,
        None,
    );

    // Assert: The typed glyph reaches the PIA, but an untyped Alt shortcut does not.
    assert_eq!(at_sign_key, 0xc0);
    assert_eq!(at_sign_modifiers, 0x08);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Text from a later key cannot make an earlier Alt shortcut look like AltGr.
// Why: Only text paired with the same key event may bypass Alt shortcut suppression.
#[test]
fn unrelated_text_does_not_allow_an_alt_shortcut() {
    // Arrange: Prepare an Alt shortcut followed by a normal key with printable text.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardInput::default();
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

    // Act: Process both key events and the later key's text in one frame.
    keyboard_input.handle_events(&mut machine, &events, no_modifiers, true, None);

    // Assert: The final PIA value is the at-sign mapping paired with B.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xc0);
}

// What: Typed characters preserve the web mapping for shifted glyphs.
// Why: A logical key alone cannot distinguish every shifted character on host layouts.
#[test]
fn text_events_map_shifted_glyphs_before_machine_input() {
    // Arrange: Configure observable PIA ports and a fresh keyboard adapter.
    let mut machine = Machine::default();
    configure_keyboard_ports(&mut machine);
    let mut keyboard_input = KeyboardInput::default();
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
    let mut keyboard_input = KeyboardInput::default();
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
