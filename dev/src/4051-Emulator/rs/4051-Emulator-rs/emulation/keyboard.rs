//! Framework-neutral host-key and printable-character translation.
//!
//! Adapters reduce platform key events to `KeyboardKey`; this module supplies
//! the Tektronix code and Shift-line behavior shared by native and browser hosts.

/// A logical host key after platform-specific event extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardKey {
    /// A printable ASCII character, including a space.
    Printable(char),
    Backspace,
    Tab,
    Enter,
    Escape,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    /// A host function key number, such as `1` for F1.
    Function(u8),
}

/// The Tek key code and Shift-line behavior for one supported host key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyMapping {
    /// The code presented to the emulated keyboard.
    pub code: u8,
    /// Whether this character reverses the host Shift line.
    pub invert_shift: bool,
    /// Whether a paired text event should select the character mapping.
    pub text_input: bool,
}

impl KeyMapping {
    const fn character(code: u8, invert_shift: bool) -> Self {
        Self {
            code,
            invert_shift,
            text_input: true,
        }
    }

    const fn panel(code: u8) -> Self {
        Self {
            code,
            invert_shift: false,
            text_input: false,
        }
    }
}

/// Maps one printable ASCII character to the reference Tek keyboard encoding.
pub fn map_text_character(character: char) -> Option<KeyMapping> {
    if !(character.is_ascii_graphic() || character == ' ') {
        return None;
    }

    let (code, invert_shift) = match character {
        ' ' => (0x10, false),
        '!' => (b'1', false),
        '"' => (b'2', false),
        '#' => (b'3', true),
        '$' => (b'4', false),
        '%' => (b'5', false),
        '&' => (b'6', false),
        '\'' => (b'7', true),
        '(' => (b'8', false),
        ')' => (b'9', false),
        '+' => (b';', false),
        ',' => (b'<', false),
        '-' => (b'=', false),
        '.' => (b'>', false),
        '/' => (b'?', false),
        ':' => (b':', true),
        '=' => (b'=', true),
        '@' => (b'@', true),
        '`' => (b'@', false),
        '{' => (b'[', false),
        '|' => (b'\\', false),
        '}' => (b']', false),
        '~' => (b'^', true),
        'a'..='z' => (character.to_ascii_uppercase() as u8, false),
        _ => (character as u8, false),
    };

    Some(KeyMapping::character(code, invert_shift))
}

/// Adjusts the printable mapping for the host's actual Shift state.
///
/// `map_text_character` uses the conventional US keyboard Shift state. This
/// variant preserves the requested character when another layout uses Shift
/// differently for that character.
pub fn map_text_character_with_host_shift(character: char, host_shift: bool) -> Option<KeyMapping> {
    let mut mapping = map_text_character(character)?;
    let us_shift = character.is_ascii_uppercase() || "!\"#$%&()*+<>?@^_{}|~:".contains(character);
    if us_shift != host_shift {
        mapping.invert_shift = !mapping.invert_shift;
    }
    Some(mapping)
}

/// Maps a platform-neutral logical host key to its Tek keyboard operation.
pub fn map_host_key(key: KeyboardKey) -> Option<KeyMapping> {
    Some(match key {
        KeyboardKey::Printable(character) => return map_text_character(character),
        KeyboardKey::Backspace => KeyMapping::panel(0x08),
        KeyboardKey::Tab => KeyMapping::panel(0x09),
        KeyboardKey::Enter => KeyMapping::panel(0x0d),
        KeyboardKey::Escape => KeyMapping::panel(0x1b),
        KeyboardKey::Delete => KeyMapping::panel(0x5f),
        KeyboardKey::Insert => KeyMapping::panel(0x73),
        KeyboardKey::Home => KeyMapping::panel(0x6a),
        KeyboardKey::End => KeyMapping::panel(0x6b),
        KeyboardKey::PageUp => KeyMapping::panel(0x75),
        KeyboardKey::Function(number) => match number {
            1..=10 => KeyMapping::panel(0x5f + number),
            13 => KeyMapping::panel(0x6a),
            14 => KeyMapping::panel(0x6b),
            15 => KeyMapping::panel(0x73),
            16 => KeyMapping::panel(0x75),
            17 => KeyMapping::panel(0x76),
            _ => return None,
        },
    })
}
