//! Visible initial-menu smoke regression for the supplied Pictures set.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Pictures and run file 1 through its initial menu/input screen.
// Why: Exercise menu startup without claiming that every picture or UDK action works.
fn main() -> std::process::ExitCode {
    // Arrange: Select the unchanged Pictures directory and R12 menu entry.
    let name = "pictures";
    // Act: Display the shared file-1 load/run sequence and input-screen smoke check.
    let result = visible::run(name);
    // Assert: Propagate startup failure instead of substituting another menu variant.
    result
}
