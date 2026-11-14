//! Visible initial-input smoke regression for the supplied Fractals set.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Fractals and run file 1 to its first input screen.
// Why: Cover startup separately from choosing sides, inverse mode and recursion levels.
fn main() -> std::process::ExitCode {
    // Arrange: Select the unchanged Fractals directory and file-1 program.
    let name = "fractals";
    // Act: Display the real-ROM boot/load/run sequence and initial-input smoke result.
    let result = visible::run(name);
    // Assert: Return the configured startup outcome without claiming fractal completion.
    result
}
