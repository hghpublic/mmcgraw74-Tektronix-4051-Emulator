//! Visible compatibility smoke regression for the supplied 4054 Option 30 entry.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Asteroids and run the user-selected file-1 4054 Option 30 program.
// Why: Expose incompatible startup rather than substituting the file-2 4051 variant.
fn main() -> std::process::ExitCode {
    // Arrange: Select the unchanged Asteroids set, including its original duplicate file numbers.
    let name = "asteroids";
    // Act: Display the load/run sequence and check running, changing stored graphics.
    let result = visible::run(name);
    // Assert: Unsupported execution or absent graphics activity remains a nonzero failure.
    result
}
