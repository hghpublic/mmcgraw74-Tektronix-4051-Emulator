// What: Publish changed framebuffer data and a newly terminal outcome into an existing snapshot.
// Why: The GUI must receive the final frame and result, not retain the worker's initial state.
#[test]
fn update_publishes_new_terminal_frame_and_outcome() {
    use super::{Session, Snapshot};

    // Arrange: Capture the imported session before any terminal result or display erase flash.
    let mut session = Session::new("simple_demo").expect("SimpleDemo assets must import");
    let mut snapshot = Snapshot::new(&mut session);
    let previous_pixels = snapshot.pixels.clone();
    let previous_revision = snapshot.revision;
    assert!(snapshot.outcome.is_none());

    // Act: Change the actual machine frame, terminate the session, and publish into the old snapshot.
    session.machine.erase_display();
    session.fail_wall_timeout();
    snapshot.update(&mut session);

    // Assert: Publication replaces both the stale image/revision and the absent terminal result.
    assert_ne!(snapshot.pixels, previous_pixels);
    assert_eq!(snapshot.pixels, session.display().pixels());
    assert_ne!(snapshot.revision, previous_revision);
    assert_eq!(snapshot.revision, session.display().revision());
    assert_eq!(snapshot.outcome, session.outcome);
    assert!(snapshot.outcome.as_ref().unwrap().is_err());
}
