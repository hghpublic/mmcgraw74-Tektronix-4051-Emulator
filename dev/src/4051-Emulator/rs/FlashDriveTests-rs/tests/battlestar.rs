//! Visible startup smoke regression for the unchanged Battlestar Flash Drive set.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Battlestar, load file 1 and observe its initial input screen.
// Why: Exercise real-ROM startup without claiming coverage of all menu scenes or music.
fn main() -> std::process::ExitCode {
    // Arrange: Select the supplied Battlestar directory and its file-1 entry.
    let name = "battlestar";
    // Act: Display the shared boot/FIND/OLD/RUN sequence and its resulting frame.
    let result = visible::run(name);
    // Assert: Preserve nonzero failures, including startup/readiness timeouts.
    result
}
