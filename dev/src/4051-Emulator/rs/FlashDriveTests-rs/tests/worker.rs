use std::{thread, time::Duration};

#[path = "support/session.rs"]
mod session;
#[path = "support/worker.rs"]
mod worker;

use session::Session;
use worker::SessionWorker;

// What: Let the background session run without any native frames or view polling.
// Why: A blocked or slow native event loop must no longer starve boot and scripted input.
#[test]
fn session_advances_without_repaints() {
    // Arrange: Start the same real-ROM worker used by the graphical test.
    let session = Session::new("simple_demo").expect("SimpleDemo assets must import");
    assert_eq!(
        session.imported_files, 7,
        "worker fixture must remain SimpleDemo"
    );
    let (worker, view) = SessionWorker::start(session).expect("worker must start");
    let started = worker.started;

    // Act: Leave the view completely unpolled, then capture progress and join the worker.
    thread::sleep(Duration::from_secs(15));
    let snapshot = view.snapshot();
    worker.finish().expect("worker must join cleanly");

    // Assert: Boot progressed into scripted commands without a single GUI callback.
    let snapshot = snapshot.expect("worker must publish its progress");
    assert!(snapshot.step >= 1, "worker did not finish boot");
    assert!(started.elapsed() < Duration::from_secs(30));
}

// What: Publish an already-terminal session without advancing its machine.
// Why: Completion-channel disconnection must retain the failure and final frame for the GUI.
#[test]
fn terminal_failure_survives_worker_completion() {
    // Arrange: Trigger the same wall-deadline failure before starting the worker.
    let mut session = Session::new("simple_demo").expect("SimpleDemo assets must import");
    session.fail_wall_timeout();
    let outcome = session.outcome.clone();
    let pixels = session.display().pixels().to_vec();
    let (worker, view) = SessionWorker::start(session).expect("worker must start");

    // Act: Join the worker before the view reads its completed state.
    worker.finish().expect("worker must join cleanly");
    let snapshot = view
        .snapshot()
        .expect("terminal result must remain readable");

    // Assert: A finished producer is not mistaken for an abandoned session or a successful test.
    assert_eq!(snapshot.outcome, outcome);
    assert_eq!(snapshot.pixels, pixels);
    assert_eq!(snapshot.step, 0);
}
