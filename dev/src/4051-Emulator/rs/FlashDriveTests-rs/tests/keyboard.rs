use std::time::Duration;

#[path = "support/keyboard.rs"]
mod keyboard;
use keyboard::{KeyEvent, KeySequence};

// What: Emit a command containing repeated digits and RETURN with complete hold/release intervals.
// Why: Releasing a key must not re-press it or prevent advancement to the next command character.
#[test]
fn repeated_characters_and_return_have_distinct_press_and_release_events() {
    // Arrange: Use the same sequencer and 10 ms machine-tick ordering as the graphical runner.
    let mut sequence = KeySequence::new(b"119\r");
    let mut events = Vec::new();
    let tick = Duration::from_millis(10);
    let mut elapsed = Duration::ZERO;

    // Act: Observe presses before each machine tick and releases/completion at its end.
    for _ in 0..90 {
        if let Some(event) = sequence.press_event() {
            events.push((elapsed.as_millis(), event));
        }
        elapsed += tick;
        if let Some(event) = sequence.advance(tick) {
            events.push((elapsed.as_millis(), event));
        }
    }

    // Assert: Repeated keys remain distinct, every gap lasts 100 ms, and completion emits only once.
    assert_eq!(
        events,
        vec![
            (0, KeyEvent::Press(b'1')),
            (100, KeyEvent::Release),
            (200, KeyEvent::Press(b'1')),
            (300, KeyEvent::Release),
            (400, KeyEvent::Press(b'9')),
            (500, KeyEvent::Release),
            (600, KeyEvent::Press(b'\r')),
            (700, KeyEvent::Release),
            (800, KeyEvent::Finished),
        ]
    );
}
