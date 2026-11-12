use crate::emulation::runtime::HostRuntime;
use crate::machine::Machine;
use std::time::Duration;

use crate::emulation::display::{DisplayEvent, DisplayEventBatch, DisplayPoint};

use super::{CURSOR_MARKS, CursorReadinessDetector, matches_input_prompt_sequence};

fn batch(events: &[DisplayEvent]) -> DisplayEventBatch {
    DisplayEventBatch {
        events: events.to_vec(),
        dropped: false,
    }
}

fn cursor_block(origin: DisplayPoint) -> Vec<DisplayEvent> {
    (0..5)
        .flat_map(|column| {
            (0..8).map(move |row| {
                DisplayEvent::TransientCursorMark(DisplayPoint {
                    x: origin.x + column * 2,
                    y: origin.y + row * 2,
                })
            })
        })
        .collect()
}

fn input_prompt_question_mark(origin: DisplayPoint) -> Vec<DisplayEvent> {
    [
        (4, 0),
        (4, 4),
        (4, 6),
        (6, 8),
        (0, 10),
        (8, 10),
        (2, 12),
        (4, 12),
        (6, 12),
    ]
    .map(|(x, y)| {
        DisplayEvent::TransientCursorMark(DisplayPoint {
            x: origin.x + x,
            y: origin.y + y,
        })
    })
    .to_vec()
}

fn type_tek_keys(machine: &mut Machine, runtime: &mut HostRuntime, keys: &[u8]) {
    for &key in keys {
        machine.keyboard_press(key);
        runtime
            .advance(machine, Duration::from_millis(100))
            .unwrap();
        machine.keyboard_release();
        runtime
            .advance(machine, Duration::from_millis(100))
            .unwrap();
    }
}

fn full_cursor_shape_detected(detector: &CursorReadinessDetector) -> bool {
    detector
        .candidate_support
        .values()
        .any(|&support| support == CURSOR_MARKS)
}

// What: A complete transient 5-by-8 cursor glyph signals BASIC readiness.
// Why: The detector must recognize the ROM's cursor shape without observing raster decay.
#[test]
fn complete_transient_cursor_glyph_signals_readiness() {
    // Arrange: Prepare one logical cursor cell and withhold its final mark.
    let origin = DisplayPoint { x: 120, y: 80 };
    let mut marks = cursor_block(origin);
    let final_mark = marks.pop().unwrap();
    let mut detector = CursorReadinessDetector::default();

    // Act: Feed the first 39 marks, then the final distinct mark.
    assert!(!detector.observe(batch(&marks)));
    let ready = detector.observe(batch(&[final_mark]));

    // Assert: Readiness follows only after all 40 positions of the cursor block appear.
    assert!(ready);
}

// What: Marks from two nearby positions do not form a complete cursor glyph.
// Why: Repeated or moved single points must not substitute for the ROM's full cursor shape.
#[test]
fn partial_or_moved_cursor_marks_do_not_signal_readiness() {
    // Arrange: Start with no accumulated transient cursor marks.
    let mut detector = CursorReadinessDetector::default();

    // Act: Observe two marks at distinct coordinates.
    let ready = detector.observe(batch(&[
        DisplayEvent::TransientCursorMark(DisplayPoint { x: 120, y: 80 }),
        DisplayEvent::TransientCursorMark(DisplayPoint { x: 122, y: 80 }),
    ]));

    // Assert: A small moving mark sequence does not satisfy the 5-by-8 shape.
    assert!(!ready);
}

// What: An erase discards partial cursor-shape evidence.
// Why: Marks from before a screen clear must not combine with a later redraw.
#[test]
fn erase_clears_the_partial_cursor_glyph() {
    // Arrange: Accumulate all but one mark for a candidate cursor cell.
    let origin = DisplayPoint { x: 120, y: 80 };
    let mut marks = cursor_block(origin);
    let final_mark = marks.pop().unwrap();
    let mut detector = CursorReadinessDetector::default();
    assert!(!detector.observe(batch(&marks)));

    // Act: Erase the partial glyph and observe only its former final mark.
    let ready = detector.observe(batch(&[DisplayEvent::Erase, final_mark]));

    // Assert: The erased candidate cannot become ready from one post-erase mark.
    assert!(!ready);
}

// What: A dropped event batch discards partial cursor-shape evidence.
// Why: An incomplete stream cannot prove that the full transient cursor was drawn.
#[test]
fn dropped_events_clear_the_partial_cursor_glyph() {
    // Arrange: Accumulate all but one mark for a candidate cursor cell.
    let origin = DisplayPoint { x: 120, y: 80 };
    let mut marks = cursor_block(origin);
    let final_mark = marks.pop().unwrap();
    let mut detector = CursorReadinessDetector::default();
    assert!(!detector.observe(batch(&marks)));

    // Act: Report event loss, then provide the one mark that previously completed the shape.
    let mut dropped_batch = batch(&[]);
    dropped_batch.dropped = true;
    assert!(!detector.observe(dropped_batch));
    let ready = detector.observe(batch(&[final_mark]));

    // Assert: Evidence from before the event loss is not reused.
    assert!(!ready);
}

// What: A permanent drawing command invalidates the previously observed cursor.
// Why: Readiness must not remain true while BASIC is drawing command output.
#[test]
fn permanent_drawing_invalidates_cursor_readiness() {
    // Arrange: Detect a complete transient cursor glyph at one logical position.
    let origin = DisplayPoint { x: 120, y: 80 };
    let mut detector = CursorReadinessDetector::default();
    assert!(detector.observe(batch(&cursor_block(origin))));

    // Act: Process a permanent dot emitted by the display command stream.
    let ready = detector.observe(batch(&[DisplayEvent::StoredMark {
        position: origin,
        size: 1,
    }]));

    // Assert: New output clears readiness until the ROM draws its cursor again.
    assert!(!ready);
}

// What: Loss of display events invalidates an already detected cursor.
// Why: Readiness cannot be trusted after a command may have been dropped.
#[test]
fn dropped_events_invalidate_cursor_readiness() {
    // Arrange: Detect a complete transient cursor glyph.
    let origin = DisplayPoint { x: 120, y: 80 };
    let mut detector = CursorReadinessDetector::default();
    assert!(detector.observe(batch(&cursor_block(origin))));

    // Act: Signal that the display event queue overflowed.
    let mut dropped_batch = batch(&[]);
    dropped_batch.dropped = true;
    let ready = detector.observe(dropped_batch);

    // Assert: The detector requires a fresh complete glyph after event loss.
    assert!(!ready);
}

// What: Recognize a transient question mark even when its marks span empty event batches.
// Why: An empty host poll does not establish that the ROM finished drawing one blink phase.
#[test]
fn input_prompt_question_mark_survives_empty_event_batches() {
    // Arrange: Split one translated question-mark glyph into three mark groups.
    let marks = input_prompt_question_mark(DisplayPoint { x: 120, y: 80 });
    let mut detector = CursorReadinessDetector::default();

    // Act: Insert empty host polls between groups, then provide the final distinct mark.
    assert!(!detector.observe(batch(&marks[..4])));
    assert!(!detector.observe(batch(&[])));
    assert!(!detector.observe(batch(&marks[4..8])));
    assert!(!detector.observe(batch(&[])));
    let ready = detector.observe(batch(&marks[8..]));

    // Assert: The full glyph signals readiness without requiring a second blink cycle.
    assert!(ready);
    assert!(detector.observe(batch(&[])));
}

// What: Clear an incomplete question-mark candidate when the display is erased.
// Why: Marks drawn after an erase must not complete a prompt shape that no longer exists.
#[test]
fn erase_discards_partial_input_prompt_question_mark() {
    // Arrange: Accumulate all but the final mark of one question-mark glyph.
    let marks = input_prompt_question_mark(DisplayPoint { x: 120, y: 80 });
    let mut detector = CursorReadinessDetector::default();
    assert!(!detector.observe(batch(&marks[..8])));

    // Act: Erase the partial glyph, then replay its former final mark before a fresh draw.
    assert!(!detector.observe(batch(&[DisplayEvent::Erase])));
    assert!(!detector.observe(batch(&marks[8..])));

    // Assert: A complete post-erase glyph is required to signal readiness.
    assert!(detector.observe(batch(&marks)));
}

// What: Invalidate question-mark readiness on permanent output and dropped display events.
// Why: Readiness cannot survive screen changes or an incomplete stream of cursor marks.
#[test]
fn stored_output_and_dropped_events_invalidate_input_prompt_readiness() {
    // Arrange: Recognize a complete transient question-mark glyph.
    let marks = input_prompt_question_mark(DisplayPoint { x: 120, y: 80 });
    let mut detector = CursorReadinessDetector::default();
    assert!(detector.observe(batch(&marks)));

    // Act: Permanent output clears readiness; a fresh glyph restores it before event loss.
    assert!(!detector.observe(batch(&[DisplayEvent::StoredMark {
        position: DisplayPoint { x: 120, y: 80 },
        size: 1,
    }])));
    assert!(detector.observe(batch(&marks)));
    let mut dropped_batch = batch(&[]);
    dropped_batch.dropped = true;
    assert!(!detector.observe(dropped_batch));

    // Assert: One post-overflow mark cannot reuse the invalidated question-mark candidate.
    assert!(!detector.observe(batch(&marks[8..])));
}

// What: Accept a real-ROM INPUT value and detect the following ordinary BASIC cursor.
// Why: The question-mark prompt must enable input, and accepted input must leave its wait loop.
#[test]
fn rom_input_prompt_accepts_value_and_returns_to_basic_cursor() {
    // Arrange: Boot the ROM and enter an INPUT/PRINT program through the machine keyboard.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut machine,
        &mut runtime,
        b"10\x10INPUT\x10A\r20\x10PRINT\x10A\r\rRUN\r",
    );
    runtime.take_display_events();
    let mut detector = CursorReadinessDetector::default();

    // Act: Wait for the ROM's transient question mark, ignoring any ordinary cursor candidate.
    let mut input_prompt_ready = false;
    for _ in 0..3_000 {
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();
        let events = runtime.take_display_events();
        if detector.observe(events) {
            input_prompt_ready = matches_input_prompt_sequence(&detector.input_prompt_marks);
            if input_prompt_ready {
                break;
            }
            detector.reset();
        }
    }
    assert!(input_prompt_ready);

    // Act: Enter 5 and Return, then wait for the full system cursor after the program completes.
    detector.reset();
    type_tek_keys(&mut machine, &mut runtime, b"5\r");
    runtime.take_display_events();
    let mut system_prompt_ready = false;
    for _ in 0..3_000 {
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();
        if detector.observe(runtime.take_display_events()) {
            system_prompt_ready = full_cursor_shape_detected(&detector);
            if system_prompt_ready {
                break;
            }
            detector.reset();
        }
    }

    // Assert: BASIC left INPUT after consuming the value and later drew its full-block cursor.
    assert!(system_prompt_ready);
}

// What: Preserve ROM-prompt readiness when a host runs the CPU at x50 or x100.
// Why: Faster execution must not make transient cursor events overflow before the host observes them.
#[test]
fn rom_cursor_readiness_survives_high_cpu_speeds() {
    // Arrange: Boot to the ROM prompt at x1, discard startup output, and detect its cursor.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();
    runtime.take_display_events();

    let mut detector = CursorReadinessDetector::default();
    let mut prompt_ready = false;
    for _ in 0..3_000 {
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();
        if detector.observe(runtime.take_display_events()) {
            prompt_ready = true;
            break;
        }
    }
    assert!(prompt_ready, "the ROM prompt did not become ready at x1");

    // Act: Run one 10 ms CPU interval at each high speed and observe its event batch.
    for speed_factor in [50.0, 100.0] {
        runtime.set_speed_factor(speed_factor).unwrap();
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();
        let events = runtime.take_display_events();

        // Assert: The cursor's repeated transient marks do not overflow the batch or lose readiness.
        assert!(
            !events.dropped,
            "display events overflowed at x{}",
            speed_factor as u32
        );
        assert!(
            detector.observe(events),
            "BASIC readiness was lost at x{}",
            speed_factor as u32
        );
    }
}
