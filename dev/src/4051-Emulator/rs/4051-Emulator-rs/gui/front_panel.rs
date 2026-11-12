//! Presents front-panel keys and exposes their clipped native hit regions.
//!
//! The shifted functions use the same Tek codes with the Shift line active.
//! The keyboard adapter interprets pointers against these regions; timing and
//! machine input ownership belong to the separate emulation input sequencer.

use eframe::egui::{self, Pos2, Rect};

#[cfg(test)]
#[path = "front_panel/tests.rs"]
mod tests;

const USER_KEY_COUNT: usize = 10;
const EDITOR_KEYS: [(&str, u8); 5] = [
    ("EXPAND / COMPRESS", 0x70),
    ("BACK SPACE / RUB OUT LEFT", 0x71),
    ("SPACE / RUB OUT RIGHT", 0x72),
    ("CLEAR / REPRINT", 0x73),
    ("RECALL LINE / RECALL NEXT LINE", 0x74),
];
const PROGRAM_KEYS: [(&str, u8); 2] = [("AUTO NUMBER", 0x75), ("STEP PROGRAM", 0x76)];
const SPECIAL_KEYS: [(&str, u8); 3] = [("HOME / PAGE", 0x6a), ("RUB OUT", 0x5f), ("BREAK", 0x6b)];
const KEY_COUNT: usize =
    USER_KEY_COUNT + EDITOR_KEYS.len() + PROGRAM_KEYS.len() + SPECIAL_KEYS.len();

pub(super) struct FrontPanel {
    key_rects: [(Rect, u8); KEY_COUNT],
}

impl Default for FrontPanel {
    fn default() -> Self {
        Self {
            key_rects: [(Rect::NOTHING, 0); KEY_COUNT],
        }
    }
}

impl FrontPanel {
    pub(super) fn key_at(&self, position: Pos2) -> Option<u8> {
        self.key_rects
            .iter()
            .find_map(|(rect, code)| rect.contains(position).then_some(*code))
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> bool {
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
        let mut copy_requested = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Program:");
            for (index, &(label, code)) in PROGRAM_KEYS.iter().enumerate() {
                let response = ui.button(label);
                self.key_rects[USER_KEY_COUNT + EDITOR_KEYS.len() + index] =
                    (response.rect.intersect(ui.clip_rect()), code);
            }
            copy_requested = ui.button("MAKE COPY").clicked();
            ui.label("Keyboard:");
            for (index, &(label, code)) in SPECIAL_KEYS.iter().enumerate() {
                let response = ui.button(label);
                self.key_rects[USER_KEY_COUNT + EDITOR_KEYS.len() + PROGRAM_KEYS.len() + index] =
                    (response.rect.intersect(ui.clip_rect()), code);
            }
        });
        copy_requested
    }
}
