use tek4051_emulator::emulation::keyboard::{
    KeyMapping, KeyboardKey, map_host_key, map_text_character, map_text_character_with_host_shift,
};

// What: Translate every printable ASCII character through the shared keyboard mapping.
// Why: Native and browser adapters must send the same Tek codes and Shift-line behavior.
#[test]
fn printable_text_matches_reference_character_mapping() {
    // Arrange: Define every non-identity glyph and Shift-line exception in printable ASCII.
    let overrides = [
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
        ('+', b';', false),
        (',', b'<', false),
        ('-', b'=', false),
        ('.', b'>', false),
        ('/', b'?', false),
        (':', b':', true),
        ('=', b'=', true),
        ('@', b'@', true),
        ('`', b'@', false),
        ('{', b'[', false),
        ('|', b'\\', false),
        ('}', b']', false),
        ('~', b'^', true),
    ];

    // Act: Translate every printable ASCII character and probe rejected boundaries.
    let actual: Vec<_> = (0x20..=0x7e)
        .map(|value| {
            let character = char::from(value);
            let expected = overrides
                .iter()
                .find(|(mapped, _, _)| *mapped == character)
                .map(|(_, code, invert_shift)| (*code, *invert_shift))
                .unwrap_or_else(|| {
                    let code = if character.is_ascii_lowercase() {
                        character.to_ascii_uppercase() as u8
                    } else {
                        value
                    };
                    (code, false)
                });
            (character, map_text_character(character), expected)
        })
        .collect();

    // Assert: Every printable glyph has its reference code; controls and Unicode are rejected.
    for (character, mapping, (code, invert_shift)) in actual {
        assert_eq!(
            mapping,
            Some(KeyMapping {
                code,
                invert_shift,
                text_input: true,
            }),
            "unexpected mapping for {character:?}"
        );
    }
    assert_eq!(map_text_character('\n'), None);
    assert_eq!(map_text_character('\x1f'), None);
    assert_eq!(map_text_character('\x7f'), None);
    assert_eq!(map_text_character('\0'), None);
    assert_eq!(map_text_character('é'), None);
}

// What: Reconcile printable mappings with the Shift state reported by a non-US layout.
// Why: The shared Tek mapping assumes conventional US Shift positions, while browser and native
// hosts report the modifier state of the user's actual keyboard layout.
#[test]
fn printable_text_mapping_adjusts_for_host_shift_state() {
    // Arrange: Pair German-layout punctuation with the Shift state used to type each character.
    let cases = [
        ('+', false, b';', true),
        ('=', true, b'=', false),
        ('<', false, b'<', true),
        (';', true, b';', true),
        ('+', true, b';', false),
        ('=', false, b'=', true),
    ];

    // Act: Translate each character with its actual host Shift modifier.
    let mappings =
        cases.map(|(character, shift, _, _)| map_text_character_with_host_shift(character, shift));

    // Assert: Codes stay tied to the intended character and inversion compensates only for
    // layouts whose Shift state differs from the conventional US position.
    for ((character, _, code, invert_shift), mapping) in cases.into_iter().zip(mappings) {
        assert_eq!(
            mapping,
            Some(KeyMapping {
                code,
                invert_shift,
                text_input: true,
            }),
            "unexpected host-Shift mapping for {character:?}"
        );
    }
}

// What: Map logical panel keys and function-key aliases independently of a UI framework.
// Why: The browser WASM build must use the same special-key codes as the native adapter.
#[test]
fn logical_panel_keys_match_reference_codes() {
    // Arrange: Enumerate each supported special key and its expected panel code.
    let mappings = [
        (KeyboardKey::Backspace, 0x08),
        (KeyboardKey::Tab, 0x09),
        (KeyboardKey::Enter, 0x0d),
        (KeyboardKey::Escape, 0x1b),
        (KeyboardKey::Delete, 0x5f),
        (KeyboardKey::Insert, 0x73),
        (KeyboardKey::Home, 0x6a),
        (KeyboardKey::End, 0x6b),
        (KeyboardKey::PageUp, 0x75),
        (KeyboardKey::Function(1), 0x60),
        (KeyboardKey::Function(2), 0x61),
        (KeyboardKey::Function(3), 0x62),
        (KeyboardKey::Function(4), 0x63),
        (KeyboardKey::Function(5), 0x64),
        (KeyboardKey::Function(6), 0x65),
        (KeyboardKey::Function(7), 0x66),
        (KeyboardKey::Function(8), 0x67),
        (KeyboardKey::Function(9), 0x68),
        (KeyboardKey::Function(10), 0x69),
        (KeyboardKey::Function(13), 0x6a),
        (KeyboardKey::Function(14), 0x6b),
        (KeyboardKey::Function(15), 0x73),
        (KeyboardKey::Function(16), 0x75),
        (KeyboardKey::Function(17), 0x76),
    ];

    // Act: Resolve every logical key and probe unsupported function/text inputs.
    let actual: Vec<_> = mappings.iter().map(|(key, _)| map_host_key(*key)).collect();
    let expected: Vec<_> = mappings
        .iter()
        .map(|(_, code)| {
            Some(KeyMapping {
                code: *code,
                invert_shift: false,
                text_input: false,
            })
        })
        .collect();

    // Assert: Supported keys match the established panel codes; unsupported inputs stay unmapped.
    assert_eq!(actual, expected);
    assert_eq!(map_host_key(KeyboardKey::Function(11)), None);
    assert_eq!(map_host_key(KeyboardKey::Function(12)), None);
    assert_eq!(map_host_key(KeyboardKey::Function(18)), None);
    assert_eq!(map_host_key(KeyboardKey::Function(35)), None);
    assert_eq!(map_host_key(KeyboardKey::Printable('\n')), None);
}
