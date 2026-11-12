//! Translates native events into ordered, framework-neutral input operations.
//!
//! Printable and front-panel key codes follow the web keyboard mapping. Host
//! key taps use the input sequencer so a down/up pair is not lost before the
//! emulated keyboard can sample it. Losing window focus requests releases and
//! clears modifiers so input cannot remain latched.
//! Text events supply layout-dependent printable characters when paired with
//! a key; otherwise supported keys use their host-key mapping.
//! Native number editing releases emulator input and suppresses host keys, but
//! still routes front-panel pointer events.
//! Native control activation keys stay owned through release without cancelling panel input.
//! Owned Space/Enter repeats are consumed before widgets run, while raw events
//! remain available for input translation.

use eframe::egui;
use tek4051_emulator::emulation::{
    input::{InputAction, InputFrame, KeyModifiers, ManualKeyId},
    keyboard::{KeyboardKey, map_host_key, map_text_character_with_host_shift},
};

use super::front_panel::FrontPanel;

#[derive(Default)]
pub(super) struct KeyboardInput {
    ui_owned_keys: Vec<egui::Key>,
}

pub(super) fn key_modifiers(modifiers: egui::Modifiers) -> KeyModifiers {
    KeyModifiers {
        shift: modifiers.shift,
        control: modifiers.ctrl,
    }
}

fn manual_key_id(key: egui::Key) -> ManualKeyId {
    ManualKeyId(key as u64)
}

fn keyboard_key(key: egui::Key) -> Option<KeyboardKey> {
    use egui::Key;

    Some(match key {
        Key::A => KeyboardKey::Printable('A'),
        Key::B => KeyboardKey::Printable('B'),
        Key::C => KeyboardKey::Printable('C'),
        Key::D => KeyboardKey::Printable('D'),
        Key::E => KeyboardKey::Printable('E'),
        Key::F => KeyboardKey::Printable('F'),
        Key::G => KeyboardKey::Printable('G'),
        Key::H => KeyboardKey::Printable('H'),
        Key::I => KeyboardKey::Printable('I'),
        Key::J => KeyboardKey::Printable('J'),
        Key::K => KeyboardKey::Printable('K'),
        Key::L => KeyboardKey::Printable('L'),
        Key::M => KeyboardKey::Printable('M'),
        Key::N => KeyboardKey::Printable('N'),
        Key::O => KeyboardKey::Printable('O'),
        Key::P => KeyboardKey::Printable('P'),
        Key::Q => KeyboardKey::Printable('Q'),
        Key::R => KeyboardKey::Printable('R'),
        Key::S => KeyboardKey::Printable('S'),
        Key::T => KeyboardKey::Printable('T'),
        Key::U => KeyboardKey::Printable('U'),
        Key::V => KeyboardKey::Printable('V'),
        Key::W => KeyboardKey::Printable('W'),
        Key::X => KeyboardKey::Printable('X'),
        Key::Y => KeyboardKey::Printable('Y'),
        Key::Z => KeyboardKey::Printable('Z'),
        Key::Num0 => KeyboardKey::Printable('0'),
        Key::Num1 => KeyboardKey::Printable('1'),
        Key::Num2 => KeyboardKey::Printable('2'),
        Key::Num3 => KeyboardKey::Printable('3'),
        Key::Num4 => KeyboardKey::Printable('4'),
        Key::Num5 => KeyboardKey::Printable('5'),
        Key::Num6 => KeyboardKey::Printable('6'),
        Key::Num7 => KeyboardKey::Printable('7'),
        Key::Num8 => KeyboardKey::Printable('8'),
        Key::Num9 => KeyboardKey::Printable('9'),
        Key::Space => KeyboardKey::Printable(' '),
        Key::Backspace => KeyboardKey::Backspace,
        Key::Tab => KeyboardKey::Tab,
        Key::Enter => KeyboardKey::Enter,
        Key::Escape => KeyboardKey::Escape,
        Key::Delete => KeyboardKey::Delete,
        Key::Insert => KeyboardKey::Insert,
        Key::Home | Key::F13 => KeyboardKey::Home,
        Key::End | Key::F14 => KeyboardKey::End,
        Key::PageUp | Key::F16 => KeyboardKey::PageUp,
        Key::F1 => KeyboardKey::Function(1),
        Key::F2 => KeyboardKey::Function(2),
        Key::F3 => KeyboardKey::Function(3),
        Key::F4 => KeyboardKey::Function(4),
        Key::F5 => KeyboardKey::Function(5),
        Key::F6 => KeyboardKey::Function(6),
        Key::F7 => KeyboardKey::Function(7),
        Key::F8 => KeyboardKey::Function(8),
        Key::F9 => KeyboardKey::Function(9),
        Key::F10 => KeyboardKey::Function(10),
        Key::F15 => KeyboardKey::Function(15),
        Key::F17 => KeyboardKey::Function(17),
        Key::Colon => KeyboardKey::Printable(':'),
        Key::Quote => KeyboardKey::Printable('\''),
        Key::Equals => KeyboardKey::Printable('='),
        Key::Comma => KeyboardKey::Printable(','),
        Key::Backslash => KeyboardKey::Printable('\\'),
        Key::Pipe => KeyboardKey::Printable('|'),
        Key::Slash => KeyboardKey::Printable('/'),
        Key::Questionmark => KeyboardKey::Printable('?'),
        Key::Exclamationmark => KeyboardKey::Printable('!'),
        Key::OpenBracket => KeyboardKey::Printable('['),
        Key::OpenCurlyBracket => KeyboardKey::Printable('{'),
        Key::CloseBracket => KeyboardKey::Printable(']'),
        Key::CloseCurlyBracket => KeyboardKey::Printable('}'),
        Key::Backtick => KeyboardKey::Printable('`'),
        Key::Minus => KeyboardKey::Printable('-'),
        Key::Period => KeyboardKey::Printable('.'),
        Key::Plus => KeyboardKey::Printable('+'),
        Key::Semicolon => KeyboardKey::Printable(';'),
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

// A filtered press also owns its next text event; intervening keys end that pairing.
fn retain_key_events(
    events: &mut Vec<egui::Event>,
    mut retain_key: impl FnMut(egui::Key, bool, bool) -> bool,
) {
    let mut owned_text = false;
    events.retain(|event| match event {
        egui::Event::Key {
            key,
            pressed,
            repeat,
            ..
        } => {
            let retain = retain_key(*key, *pressed, *repeat);
            owned_text = *pressed && !retain;
            retain
        }
        egui::Event::Text(_) if owned_text => {
            owned_text = false;
            false
        }
        _ => true,
    });
}

impl KeyboardInput {
    pub(super) fn consume_ui_activation_repeats(&self, context: &egui::Context) {
        context.input_mut(|input| {
            // Widget events are separate from raw events used for translation.
            retain_key_events(&mut input.events, |key, pressed, repeat| {
                !(pressed
                    && repeat
                    && matches!(key, egui::Key::Space | egui::Key::Enter)
                    && self.ui_owned_keys.contains(&key))
            });
        });
    }

    pub(super) fn consume_directory_navigation(&self, context: &egui::Context, list_focused: bool) {
        if list_focused {
            // Preserve directory ownership when egui moves widget focus during key handling.
            context.input_mut(|input| {
                retain_key_events(&mut input.events, |key, _, _| {
                    !matches!(
                        key,
                        egui::Key::ArrowUp
                            | egui::Key::ArrowDown
                            | egui::Key::Enter
                            | egui::Key::Space
                    )
                });
            });
        }
    }

    /// Resolves native ownership and layout before input crosses the worker boundary.
    pub(super) fn translate_frame(
        &mut self,
        panel: &FrontPanel,
        input: (&[egui::Event], egui::Modifiers, bool),
        text_editing: bool,
        owned_keys: &[egui::Key],
    ) -> InputFrame {
        let (events, mut modifiers, focused) = input;
        let mut events = events.to_vec();
        if !focused {
            self.ui_owned_keys.clear();
            return self.translate_events(&[], egui::Modifiers::NONE, false, None);
        }
        let mut actions = Vec::new();
        // Keep paired text and later releases out of BASIC even after widget focus moves.
        retain_key_events(&mut events, |key, pressed, _| {
            if owned_keys.contains(&key) || self.ui_owned_keys.contains(&key) {
                actions.push(InputAction::SequencedManualRelease(manual_key_id(key)));
                if pressed {
                    if !self.ui_owned_keys.contains(&key) {
                        self.ui_owned_keys.push(key);
                    }
                } else {
                    self.ui_owned_keys.retain(|owned| *owned != key);
                }
                false
            } else {
                true
            }
        });
        if text_editing {
            events.retain(|event| !matches!(event, egui::Event::Key { .. } | egui::Event::Text(_)));
            modifiers = egui::Modifiers::NONE;
        }

        let mut frame = self.translate_events(&events, modifiers, focused, Some(panel));
        actions.append(&mut frame.actions);
        frame.actions = actions;
        frame.reset_input = text_editing;
        frame
    }

    fn translate_events(
        &self,
        events: &[egui::Event],
        modifiers: egui::Modifiers,
        focused: bool,
        panel: Option<&FrontPanel>,
    ) -> InputFrame {
        if !focused {
            return InputFrame::unfocused();
        }
        let mut actions = Vec::new();

        // Pair text with the preceding printable key; another key ends the pairing.
        let mut typed_characters = vec![None; events.len()];
        let mut pending_text_key = None;
        for (index, event) in events.iter().enumerate() {
            match event {
                egui::Event::Key {
                    key, pressed: true, ..
                } => {
                    pending_text_key = keyboard_key(*key)
                        .and_then(map_host_key)
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
                if *pressed && *repeat {
                    continue;
                }
                actions.push(InputAction::Modifiers(key_modifiers(*modifiers)));
                if *pressed {
                    let typed_character = typed_characters[index];
                    if modifiers.mac_cmd || (modifiers.alt && typed_character.is_none()) {
                        continue;
                    }
                    if let Some(mut mapped_key) = keyboard_key(*key).and_then(map_host_key) {
                        if let Some(typed_character) = typed_character {
                            let Some(typed_character) = typed_character else {
                                continue;
                            };
                            let Some(typed_key) = map_text_character_with_host_shift(
                                typed_character,
                                modifiers.shift,
                            ) else {
                                continue;
                            };
                            mapped_key = typed_key;
                        }
                        actions.push(InputAction::SequencedManualPress {
                            key: manual_key_id(*key),
                            code: mapped_key.code,
                            invert_shift: mapped_key.invert_shift,
                            modifiers: key_modifiers(*modifiers),
                        });
                    }
                } else {
                    actions.push(InputAction::SequencedManualRelease(manual_key_id(*key)));
                }
            } else if let Some(panel) = panel {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers,
                    } if *button == egui::PointerButton::Primary => {
                        if *pressed {
                            if let Some(code) = panel.key_at(*pos) {
                                actions.push(InputAction::PanelPress {
                                    code,
                                    modifiers: key_modifiers(*modifiers),
                                });
                            }
                        } else {
                            actions.push(InputAction::PanelRelease(key_modifiers(*modifiers)));
                        }
                    }
                    egui::Event::PointerGone => {
                        actions.push(InputAction::PanelRelease(key_modifiers(modifiers)));
                    }
                    _ => {}
                }
            }
        }

        InputFrame {
            actions,
            modifiers: key_modifiers(modifiers),
            focused: true,
            reset_input: false,
        }
    }
}

fn single_ascii_character(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let character = characters.next()?;
    (character.is_ascii() && characters.next().is_none()).then_some(character)
}

#[cfg(test)]
#[path = "keyboard_input/tests.rs"]
mod tests;
