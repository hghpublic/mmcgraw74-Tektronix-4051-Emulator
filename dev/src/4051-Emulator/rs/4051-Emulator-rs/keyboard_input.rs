//! Translates host keyboard events into the Tektronix keyboard PIA interface.
//!
//! Printable and front-panel key codes follow the web keyboard mapping. The
//! adapter tracks one held key at a time; losing window focus releases it and
//! clears modifier lines so input cannot remain latched.
//! Text events supply layout-dependent printable characters when paired with
//! a key; otherwise supported keys use their host-key mapping.

use eframe::egui;
use tek4051_emulator::machine::{KeyboardModifier, Machine};

use crate::front_panel::FrontPanel;

#[derive(Default)]
pub(super) struct KeyboardInput {
    held_key: Option<(egui::Key, bool)>,
    panel_owns_key: bool,
    panel_modifiers: Option<egui::Modifiers>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MappedKey {
    code: u8,
    invert_shift: bool,
    text_input: bool,
}

impl MappedKey {
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

impl KeyboardInput {
    pub(super) fn handle_frame(
        &mut self,
        machine: &mut Machine,
        context: &egui::Context,
        panel: &mut FrontPanel,
    ) {
        let (events, modifiers, focused) =
            context.input(|input| (input.raw.events.clone(), input.modifiers, input.focused));
        self.handle_events(machine, &events, modifiers, focused, Some(panel));
    }

    fn handle_events(
        &mut self,
        machine: &mut Machine,
        events: &[egui::Event],
        modifiers: egui::Modifiers,
        focused: bool,
        mut panel: Option<&mut FrontPanel>,
    ) {
        if !focused {
            self.release_held_key(machine);
            self.release_panel_key(machine);
            self.apply_modifiers(machine, egui::Modifiers::NONE);
            if let Some(panel) = panel {
                panel.cancel(self, machine);
            }
            return;
        }

        // Pair text with the preceding printable key; another key ends the pairing.
        let mut typed_characters = vec![None; events.len()];
        let mut pending_text_key = None;
        for (index, event) in events.iter().enumerate() {
            match event {
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    pending_text_key = map_host_key(*key)
                        .filter(|mapped_key| mapped_key.text_input)
                        .map(|_| index);
                }
                egui::Event::Key { .. } => pending_text_key = None,
                egui::Event::Text(text) => {
                    if let Some(key_index) = pending_text_key.take() {
                        typed_characters[key_index] = Some(single_ascii_character(text));
                    }
                }
                _ => {}
            }
        }

        // Apply each key's event-time modifiers before reconciling frame modifiers.
        for (index, event) in events.iter().enumerate() {
            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
            {
                self.apply_modifiers(machine, *modifiers);
                if *pressed {
                    let typed_character = typed_characters[index];
                    if *repeat || modifiers.mac_cmd || (modifiers.alt && typed_character.is_none())
                    {
                        continue;
                    }
                    if let Some(mut mapped_key) = map_host_key(*key) {
                        if let Some(typed_character) = typed_character {
                            let Some(typed_character) = typed_character else {
                                continue;
                            };
                            let Some(typed_key) = map_text_character(typed_character) else {
                                continue;
                            };
                            mapped_key = typed_key;
                        }
                        if let Some(panel) = panel.as_deref_mut() {
                            panel.cancel(self, machine);
                        }
                        machine.keyboard_press(mapped_key.code);
                        self.held_key = Some((*key, mapped_key.invert_shift));
                        self.panel_owns_key = false;
                        self.panel_modifiers = None;
                        self.apply_modifiers(machine, *modifiers);
                    }
                } else if self.held_key.is_some_and(|(held_key, _)| held_key == *key) {
                    self.release_held_key(machine);
                }
            } else if let Some(panel) = panel.as_deref_mut() {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers,
                    } => panel.pointer_event(*pos, *button, *pressed, *modifiers, self, machine),
                    egui::Event::PointerGone => panel.pointer_gone(self, machine, modifiers),
                    _ => {}
                }
            }
        }

        self.apply_modifiers(machine, modifiers);
    }

    pub(super) fn set_panel_key(
        &mut self,
        machine: &mut Machine,
        code: Option<u8>,
        modifiers: egui::Modifiers,
    ) {
        if let Some(code) = code {
            machine.keyboard_press(code);
            self.held_key = None;
            self.panel_owns_key = true;
            self.panel_modifiers = None;
        } else {
            self.release_panel_key(machine);
        }
        self.apply_modifiers(machine, modifiers);
    }

    fn release_panel_key(&mut self, machine: &mut Machine) {
        if self.panel_owns_key {
            machine.keyboard_release();
            self.panel_owns_key = false;
        }
        self.panel_modifiers = None;
    }

    pub(super) fn hold_panel_modifiers(
        &mut self,
        machine: &mut Machine,
        held: egui::Modifiers,
        current: egui::Modifiers,
    ) {
        if self.panel_owns_key {
            self.panel_modifiers = Some(held);
            self.apply_modifiers(machine, current);
        }
    }

    fn release_held_key(&mut self, machine: &mut Machine) {
        if self.held_key.take().is_some() {
            machine.keyboard_release();
        }
    }

    fn apply_modifiers(&self, machine: &mut Machine, modifiers: egui::Modifiers) {
        let modifiers = self.panel_modifiers.unwrap_or(modifiers);
        let invert_shift = self.held_key.is_some_and(|(_, invert_shift)| invert_shift);
        machine.set_keyboard_modifier(KeyboardModifier::Shift, modifiers.shift ^ invert_shift);
        machine.set_keyboard_modifier(KeyboardModifier::Control, modifiers.ctrl);
    }
}

fn single_ascii_character(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let character = characters.next()?;
    (character.is_ascii() && characters.next().is_none()).then_some(character)
}

// Mirrors the web reference's character lookup and selected Shift-line inversions.
fn map_text_character(character: char) -> Option<MappedKey> {
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

    Some(MappedKey::character(code, invert_shift))
}

fn map_host_key(key: egui::Key) -> Option<MappedKey> {
    use egui::Key;

    Some(match key {
        Key::A => MappedKey::character(b'A', false),
        Key::B => MappedKey::character(b'B', false),
        Key::C => MappedKey::character(b'C', false),
        Key::D => MappedKey::character(b'D', false),
        Key::E => MappedKey::character(b'E', false),
        Key::F => MappedKey::character(b'F', false),
        Key::G => MappedKey::character(b'G', false),
        Key::H => MappedKey::character(b'H', false),
        Key::I => MappedKey::character(b'I', false),
        Key::J => MappedKey::character(b'J', false),
        Key::K => MappedKey::character(b'K', false),
        Key::L => MappedKey::character(b'L', false),
        Key::M => MappedKey::character(b'M', false),
        Key::N => MappedKey::character(b'N', false),
        Key::O => MappedKey::character(b'O', false),
        Key::P => MappedKey::character(b'P', false),
        Key::Q => MappedKey::character(b'Q', false),
        Key::R => MappedKey::character(b'R', false),
        Key::S => MappedKey::character(b'S', false),
        Key::T => MappedKey::character(b'T', false),
        Key::U => MappedKey::character(b'U', false),
        Key::V => MappedKey::character(b'V', false),
        Key::W => MappedKey::character(b'W', false),
        Key::X => MappedKey::character(b'X', false),
        Key::Y => MappedKey::character(b'Y', false),
        Key::Z => MappedKey::character(b'Z', false),
        Key::Num0 => MappedKey::character(b'0', false),
        Key::Num1 => MappedKey::character(b'1', false),
        Key::Num2 => MappedKey::character(b'2', false),
        Key::Num3 => MappedKey::character(b'3', false),
        Key::Num4 => MappedKey::character(b'4', false),
        Key::Num5 => MappedKey::character(b'5', false),
        Key::Num6 => MappedKey::character(b'6', false),
        Key::Num7 => MappedKey::character(b'7', false),
        Key::Num8 => MappedKey::character(b'8', false),
        Key::Num9 => MappedKey::character(b'9', false),
        Key::Space => MappedKey::character(0x10, false),
        Key::Backspace => MappedKey::panel(0x08),
        Key::Tab => MappedKey::panel(0x09),
        Key::Enter => MappedKey::panel(0x0d),
        Key::Escape => MappedKey::panel(0x1b),
        Key::Delete => MappedKey::panel(0x5f),
        Key::Insert => MappedKey::panel(0x73),
        Key::Home | Key::F13 => MappedKey::panel(0x6a),
        Key::End | Key::F14 => MappedKey::panel(0x6b),
        Key::PageUp | Key::F16 => MappedKey::panel(0x75),
        Key::F17 => MappedKey::panel(0x76),
        Key::F1 => MappedKey::panel(0x60),
        Key::F2 => MappedKey::panel(0x61),
        Key::F3 => MappedKey::panel(0x62),
        Key::F4 => MappedKey::panel(0x63),
        Key::F5 => MappedKey::panel(0x64),
        Key::F6 => MappedKey::panel(0x65),
        Key::F7 => MappedKey::panel(0x66),
        Key::F8 => MappedKey::panel(0x67),
        Key::F9 => MappedKey::panel(0x68),
        Key::F10 => MappedKey::panel(0x69),
        Key::F15 => MappedKey::panel(0x73),
        Key::Colon => MappedKey::character(b':', true),
        Key::Quote => MappedKey::character(b'7', true),
        Key::Equals => MappedKey::character(b'=', true),
        Key::Comma => MappedKey::character(b'<', false),
        Key::Backslash | Key::Pipe => MappedKey::character(b'\\', false),
        Key::Slash | Key::Questionmark => MappedKey::character(b'?', false),
        Key::Exclamationmark => MappedKey::character(b'1', false),
        Key::OpenBracket | Key::OpenCurlyBracket => MappedKey::character(b'[', false),
        Key::CloseBracket | Key::CloseCurlyBracket => MappedKey::character(b']', false),
        Key::Backtick => MappedKey::character(b'@', false),
        Key::Minus => MappedKey::character(b'=', false),
        Key::Period => MappedKey::character(b'>', false),
        Key::Plus | Key::Semicolon => MappedKey::character(b';', false),
        Key::ArrowDown
        | Key::ArrowLeft
        | Key::ArrowRight
        | Key::ArrowUp
        | Key::Copy
        | Key::Cut
        | Key::F11
        | Key::F12
        | Key::F18
        | Key::F19
        | Key::F20
        | Key::F21
        | Key::F22
        | Key::F23
        | Key::F24
        | Key::F25
        | Key::F26
        | Key::F27
        | Key::F28
        | Key::F29
        | Key::F30
        | Key::F31
        | Key::F32
        | Key::F33
        | Key::F34
        | Key::F35
        | Key::BrowserBack
        | Key::PageDown
        | Key::Paste => return None,
    })
}

#[cfg(test)]
mod tests;
