//! Presents user-definable, line-editor, and program keys and schedules pointer input.
//!
//! The shifted functions use the same Tek codes with the Shift line active.
//! Short presses and releases are extended so keyboard callbacks can observe
//! distinct transitions even when several pointer events arrive in one frame.

use std::{collections::VecDeque, time::Duration};

use eframe::egui::{self, PointerButton, Pos2, Rect};
use tek4051_emulator::{
    host_runtime::{HostRuntime, HostRuntimeError},
    machine::Machine,
};

use crate::keyboard_input::KeyboardInput;

#[cfg(test)]
#[path = "front_panel/tests.rs"]
mod tests;

const MINIMUM_PRESS: Duration = Duration::from_millis(100);
const MINIMUM_RELEASE: Duration = Duration::from_millis(100);
const USER_KEY_COUNT: usize = 10;
const EDITOR_KEYS: [(&str, u8); 5] = [
    ("EXPAND / COMPRESS", 0x70),
    ("BACK SPACE / RUB OUT LEFT", 0x71),
    ("SPACE / RUB OUT RIGHT", 0x72),
    ("CLEAR / REPRINT", 0x73),
    ("RECALL LINE / RECALL NEXT LINE", 0x74),
];
const PROGRAM_KEYS: [(&str, u8); 2] = [("AUTO NUMBER", 0x75), ("STEP PROGRAM", 0x76)];

struct Press {
    id: u64,
    code: u8,
    modifiers: egui::Modifiers,
    released: bool,
}

struct ActivePress {
    press: Press,
    remaining: Duration,
}

pub(super) struct FrontPanel {
    key_rects: [(Rect, u8); USER_KEY_COUNT + EDITOR_KEYS.len() + PROGRAM_KEYS.len()],
    pointer_id: Option<u64>,
    next_id: u64,
    active: Option<ActivePress>,
    queued: VecDeque<Press>,
    release_remaining: Duration,
}

impl Default for FrontPanel {
    fn default() -> Self {
        Self {
            key_rects: [(Rect::NOTHING, 0);
                USER_KEY_COUNT + EDITOR_KEYS.len() + PROGRAM_KEYS.len()],
            pointer_id: None,
            next_id: 0,
            active: None,
            queued: VecDeque::new(),
            release_remaining: Duration::ZERO,
        }
    }
}

impl FrontPanel {
    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("User keys (hold Shift for 11–20):");
            for index in 0..10_u8 {
                let response = ui.button(format!("{} / {}", index + 1, index + 11));
                self.key_rects[usize::from(index)] =
                    (response.rect.intersect(ui.clip_rect()), 0x60 + index);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Line editor (hold Shift for second function):");
            for (index, &(label, code)) in EDITOR_KEYS.iter().enumerate() {
                let response = ui.button(label);
                self.key_rects[USER_KEY_COUNT + index] =
                    (response.rect.intersect(ui.clip_rect()), code);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Program:");
            for (index, &(label, code)) in PROGRAM_KEYS.iter().enumerate() {
                let response = ui.button(label);
                self.key_rects[USER_KEY_COUNT + EDITOR_KEYS.len() + index] =
                    (response.rect.intersect(ui.clip_rect()), code);
            }
        });
    }

    pub(super) fn pointer_event(
        &mut self,
        position: Pos2,
        button: PointerButton,
        pressed: bool,
        modifiers: egui::Modifiers,
        keyboard: &mut KeyboardInput,
        machine: &mut Machine,
    ) {
        if button != PointerButton::Primary {
            return;
        }
        if pressed {
            if self.pointer_id.is_some() {
                return;
            }
            if let Some(&(_, code)) = self
                .key_rects
                .iter()
                .find(|(rect, _)| rect.contains(position))
            {
                let id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1);
                self.pointer_id = Some(id);
                self.queued.push_back(Press {
                    id,
                    code,
                    modifiers,
                    released: false,
                });
                self.start_next(keyboard, machine, modifiers);
            }
        } else if let Some(id) = self.pointer_id.take() {
            if let Some(active) = self.active.as_mut().filter(|active| active.press.id == id) {
                active.press.released = true;
                active.press.modifiers = modifiers;
                keyboard.hold_panel_modifiers(machine, modifiers, modifiers);
                if active.remaining.is_zero() {
                    self.finish_press(keyboard, machine, modifiers);
                }
            } else if let Some(press) = self.queued.iter_mut().find(|press| press.id == id) {
                press.released = true;
                press.modifiers = modifiers;
            }
        }
    }

    pub(super) fn cancel(&mut self, keyboard: &mut KeyboardInput, machine: &mut Machine) {
        self.pointer_id = None;
        self.active = None;
        self.queued.clear();
        self.release_remaining = Duration::ZERO;
        keyboard.set_panel_key(machine, None, egui::Modifiers::NONE);
    }

    pub(super) fn pointer_gone(
        &mut self,
        keyboard: &mut KeyboardInput,
        machine: &mut Machine,
        modifiers: egui::Modifiers,
    ) {
        self.pointer_event(
            Pos2::ZERO,
            PointerButton::Primary,
            false,
            modifiers,
            keyboard,
            machine,
        );
    }

    pub(super) fn advance(
        &mut self,
        elapsed: Duration,
        keyboard: &mut KeyboardInput,
        machine: &mut Machine,
        runtime: &mut HostRuntime,
        modifiers: egui::Modifiers,
    ) -> Result<(), HostRuntimeError> {
        let mut remaining = elapsed;
        while !remaining.is_zero() {
            self.start_next(keyboard, machine, modifiers);
            let step = if let Some(active) = &self.active {
                if active.remaining.is_zero() {
                    remaining
                } else {
                    remaining.min(active.remaining)
                }
            } else if !self.release_remaining.is_zero() {
                remaining.min(self.release_remaining)
            } else {
                remaining
            };
            runtime.advance(machine, step)?;
            remaining -= step;

            if let Some(active) = &mut self.active {
                active.remaining = active.remaining.saturating_sub(step);
                if active.remaining.is_zero() && active.press.released {
                    self.finish_press(keyboard, machine, modifiers);
                }
            } else {
                self.release_remaining = self.release_remaining.saturating_sub(step);
            }
        }
        self.start_next(keyboard, machine, modifiers);
        Ok(())
    }

    fn start_next(
        &mut self,
        keyboard: &mut KeyboardInput,
        machine: &mut Machine,
        current_modifiers: egui::Modifiers,
    ) {
        if self.active.is_some() || !self.release_remaining.is_zero() {
            return;
        }
        if let Some(press) = self.queued.pop_front() {
            let modifiers = if press.released {
                press.modifiers
            } else {
                current_modifiers
            };
            keyboard.set_panel_key(machine, Some(press.code), modifiers);
            if press.released {
                keyboard.hold_panel_modifiers(machine, press.modifiers, press.modifiers);
            }
            self.active = Some(ActivePress {
                press,
                remaining: MINIMUM_PRESS,
            });
        }
    }

    fn finish_press(
        &mut self,
        keyboard: &mut KeyboardInput,
        machine: &mut Machine,
        modifiers: egui::Modifiers,
    ) {
        keyboard.set_panel_key(machine, None, modifiers);
        self.active = None;
        self.release_remaining = MINIMUM_RELEASE;
    }
}
