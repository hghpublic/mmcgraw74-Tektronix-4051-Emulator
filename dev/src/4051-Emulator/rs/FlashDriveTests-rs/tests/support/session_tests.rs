// What: Qualify input and completed-drawing observations without accepting stale or erased frames.
// Why: Input may retain BUSY, but wrong lamps, unchanged output and insufficient pixels must not pass.
#[test]
fn smoke_observations_require_changed_output_and_the_correct_lamps() {
    use super::{Check, MachineIndicators, Session};
    use std::time::Duration;

    // Arrange: Capture a pre-RUN fingerprint and supply a later stored-screen observation.
    let mut session = Session::new("benchmark").expect("Benchmark must import");
    session.before_run = Some(11);
    session.step_elapsed = Duration::from_secs(1);
    let input = MachineIndicators {
        busy: true,
        io: true,
        break_active: false,
    };
    let idle = MachineIndicators::default();

    // Act: Evaluate the same smoke guards used by real-ROM ticks, including INPUT with BUSY lit.
    let ready_input = session.smoke_ready(Check::InputScreen, (501, 12), input, false);
    let ready_drawing = session.smoke_ready(Check::FinishedDrawing, (501, 12), idle, false);

    // Assert: Valid observations pass; unchanged, undersized, erased and wrong-lamp states do not.
    assert!(ready_input);
    assert!(ready_drawing);
    assert!(!session.smoke_ready(Check::InputScreen, (501, 11), input, false));
    assert!(!session.smoke_ready(Check::InputScreen, (500, 12), input, false));
    assert!(!session.smoke_ready(Check::InputScreen, (501, 12), input, true));
    assert!(!session.smoke_ready(Check::InputScreen, (501, 12), idle, false));
    assert!(!session.smoke_ready(Check::FinishedDrawing, (501, 12), input, false));
    session.step_elapsed = Duration::from_millis(999);
    assert!(!session.smoke_ready(Check::FinishedDrawing, (501, 12), idle, false));
}

// What: Observe repeated and distinct animation masks, then interrupt their qualifying lamp state.
// Why: A/B/A is not three distinct post-RUN drawings, and interrupted activity must not retain credit.
#[test]
fn animation_requires_three_distinct_post_run_masks_without_interruption() {
    use super::{Check, MachineIndicators, Session};
    use std::time::Duration;

    // Arrange: Start with a captured pre-RUN mask and the running, non-input lamp state.
    let mut session = Session::new("asteroids").expect("Asteroids must import");
    session.before_run = Some(0);
    session.step_elapsed = Duration::from_secs(1);
    let running = MachineIndicators {
        busy: true,
        io: false,
        break_active: false,
    };

    // Act: Supply the baseline, then A/B/A rather than three genuinely different drawings.
    for fingerprint in [0, 1, 2, 1] {
        session.observe_drawing((600, fingerprint), running, false);
    }

    // Assert: Baseline/repeated masks do not count; a third distinct mask passes until interrupted.
    assert_eq!(session.drawing_masks, [1, 2]);
    assert!(!session.smoke_ready(Check::AnimatedDrawing, (600, 1), running, false));
    session.observe_drawing((600, 3), running, false);
    assert!(session.smoke_ready(Check::AnimatedDrawing, (600, 3), running, false));
    assert!(!session.smoke_ready(Check::AnimatedDrawing, (600, 0), running, false));
    session.observe_drawing((600, 4), MachineIndicators::default(), false);
    assert!(session.drawing_masks.is_empty());
    assert!(!session.smoke_ready(Check::AnimatedDrawing, (600, 4), running, false));
}
