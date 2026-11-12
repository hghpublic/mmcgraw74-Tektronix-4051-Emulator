//! Detects BASIC readiness from the ROM's transient cursor and INPUT-prompt marks.
//!
//! A complete 5-by-8 cursor glyph signals the ordinary BASIC prompt. Program
//! INPUT waits draw a distinct transient question mark, recognized from its
//! complete mark pattern. Readiness stays latched
//! through the blink-off phase and is invalidated by permanent drawing, vectors,
//! erase, or session input transitions; display decay remains renderer-owned.

use std::collections::{HashMap, HashSet, VecDeque};

use super::display::{DisplayEvent, DisplayEventBatch, DisplayPoint};

const CURSOR_COLUMNS: i32 = 5;
const CURSOR_ROWS: i32 = 8;
const MARK_SPACING: i32 = 2;
const MAX_CANDIDATE_MARKS: usize = 256;
const CURSOR_MARKS: u8 = (CURSOR_COLUMNS * CURSOR_ROWS) as u8;
const INPUT_PROMPT_MARK_OFFSETS: [(i32, i32); 9] = [
    (4, 0),
    (4, 4),
    (4, 6),
    (6, 8),
    (0, 10),
    (8, 10),
    (2, 12),
    (4, 12),
    (6, 12),
];

#[derive(Default)]
pub(super) struct CursorReadinessDetector {
    cursor_marks: HashSet<DisplayPoint>,
    candidate_support: HashMap<DisplayPoint, u8>,
    input_prompt_marks: VecDeque<DisplayPoint>,
    ready: bool,
}

impl CursorReadinessDetector {
    pub(super) fn observe(&mut self, batch: DisplayEventBatch) -> bool {
        if batch.dropped {
            self.clear_candidate();
            self.ready = false;
            return self.ready;
        }

        for event in batch.events {
            match event {
                DisplayEvent::TransientCursorMark(position) => {
                    if !self.ready {
                        self.observe_input_prompt_mark(position);
                        self.observe_cursor_mark(position);
                    }
                }
                DisplayEvent::StoredMark { .. }
                | DisplayEvent::TransientVector { .. }
                | DisplayEvent::StoredVector { .. }
                | DisplayEvent::Erase => {
                    self.clear_candidate();
                    self.ready = false;
                }
            }
        }

        self.ready
    }

    fn observe_input_prompt_mark(&mut self, position: DisplayPoint) {
        if self.input_prompt_marks.contains(&position) {
            return;
        }
        self.input_prompt_marks.push_back(position);
        if self.input_prompt_marks.len() > INPUT_PROMPT_MARK_OFFSETS.len() {
            self.input_prompt_marks.pop_front();
        }
        if matches_input_prompt_sequence(&self.input_prompt_marks) {
            self.ready = true;
        }
    }

    fn observe_cursor_mark(&mut self, position: DisplayPoint) {
        if self.ready || !self.cursor_marks.insert(position) {
            return;
        }

        if self.cursor_marks.len() > MAX_CANDIDATE_MARKS {
            self.clear_candidate();
            self.cursor_marks.insert(position);
        }

        for column in 0..CURSOR_COLUMNS {
            for row in 0..CURSOR_ROWS {
                let origin = DisplayPoint {
                    x: position.x - column * MARK_SPACING,
                    y: position.y - row * MARK_SPACING,
                };
                let support = self.candidate_support.entry(origin).or_default();
                *support += 1;
                if *support == CURSOR_MARKS {
                    self.ready = true;
                    return;
                }
            }
        }
    }

    fn clear_candidate(&mut self) {
        self.cursor_marks.clear();
        self.candidate_support.clear();
        self.input_prompt_marks.clear();
    }

    pub(super) fn is_ready(&self) -> bool {
        self.ready
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

fn matches_input_prompt_sequence(marks: &VecDeque<DisplayPoint>) -> bool {
    if marks.len() != INPUT_PROMPT_MARK_OFFSETS.len() {
        return false;
    }

    let origin_x = marks.iter().map(|position| position.x).min();
    let origin_y = marks.iter().map(|position| position.y).min();
    let (Some(origin_x), Some(origin_y)) = (origin_x, origin_y) else {
        return false;
    };

    INPUT_PROMPT_MARK_OFFSETS.iter().all(|&(x, y)| {
        let Some(x) = origin_x.checked_add(x) else {
            return false;
        };
        let Some(y) = origin_y.checked_add(y) else {
            return false;
        };
        marks.contains(&DisplayPoint { x, y })
    })
}

#[cfg(test)]
#[path = "cursor_readiness_tests.rs"]
mod tests;
