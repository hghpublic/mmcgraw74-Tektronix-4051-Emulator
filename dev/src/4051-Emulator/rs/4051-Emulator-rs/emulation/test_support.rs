//! Deterministic fixtures for shared-session tests.

use crate::emulation::display::{DisplayEvent, DisplayEventBatch, DisplayPoint};

use super::EmulationState;

pub(crate) fn observe_cursor_glyph(state: &mut EmulationState) {
    let events = (0..5)
        .flat_map(|column| {
            (0..8).map(move |row| {
                DisplayEvent::TransientCursorMark(DisplayPoint {
                    x: 16 + column * 2,
                    y: 755 + row * 2,
                })
            })
        })
        .collect();
    let ready = state.data.cursor_readiness.observe(DisplayEventBatch {
        events,
        dropped: false,
    });
    state.data.basic_prompt_ready = ready;
    assert!(
        ready,
        "the semantic cursor glyph should satisfy the detector"
    );
}
