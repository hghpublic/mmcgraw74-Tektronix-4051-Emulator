//! Deterministic press, hold, and release gaps shared by the graphical test and its regression.

use std::time::Duration;

const KEY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum KeyEvent {
    Press(u8),
    Release,
    Finished,
}

enum Phase {
    Ready,
    Held,
    Gap,
    Done,
}

pub(super) struct KeySequence {
    keys: &'static [u8],
    position: usize,
    phase: Phase,
    elapsed: Duration,
}

impl KeySequence {
    pub(super) fn new(keys: &'static [u8]) -> Self {
        Self {
            keys,
            position: 0,
            phase: if keys.is_empty() {
                Phase::Done
            } else {
                Phase::Ready
            },
            elapsed: Duration::ZERO,
        }
    }

    pub(super) fn press_event(&mut self) -> Option<KeyEvent> {
        if matches!(self.phase, Phase::Ready) {
            self.phase = Phase::Held;
            Some(KeyEvent::Press(self.keys[self.position]))
        } else {
            None
        }
    }

    // The caller advances its machine before applying these interval-boundary events.
    // Ready is reached only after a complete release gap, never directly after key-up.
    pub(super) fn advance(&mut self, elapsed: Duration) -> Option<KeyEvent> {
        if !matches!(self.phase, Phase::Held | Phase::Gap) {
            return None;
        }
        self.elapsed += elapsed;
        if self.elapsed < KEY_INTERVAL {
            return None;
        }
        self.elapsed = Duration::ZERO;
        match self.phase {
            Phase::Held => {
                self.phase = Phase::Gap;
                Some(KeyEvent::Release)
            }
            Phase::Gap => {
                self.position += 1;
                if self.position == self.keys.len() {
                    self.phase = Phase::Done;
                    Some(KeyEvent::Finished)
                } else {
                    self.phase = Phase::Ready;
                    None
                }
            }
            Phase::Ready | Phase::Done => unreachable!(),
        }
    }
}
