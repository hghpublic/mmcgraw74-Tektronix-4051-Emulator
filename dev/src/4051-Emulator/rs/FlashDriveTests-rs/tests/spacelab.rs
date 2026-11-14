//! Visible compatibility smoke regression for the supplied Spacelab viewer.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Spacelab and run its file-1 viewer toward its first input screen.
// Why: Expose the supplied ROOT/file-123 dependency without rewriting directory commands or assets.
fn main() -> std::process::ExitCode {
    // Arrange: Select only the unchanged Spacelab directory and its original file-1 program.
    let name = "spacelab";
    // Act: Display the real-ROM startup sequence, including any dependency/runtime failure.
    let result = visible::run(name);
    // Assert: Do not silently supply missing ROOT data or skip an incompatible workflow.
    result
}
