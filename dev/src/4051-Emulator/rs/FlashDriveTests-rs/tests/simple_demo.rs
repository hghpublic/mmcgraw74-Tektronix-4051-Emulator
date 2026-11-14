//! Visible SimpleDemo regression using the shared real-ROM session and window.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Show the original SimpleDemo interactive sequence in a native window.
// Why: Keep the established multi-stage regression while sharing native presentation.
fn main() -> std::process::ExitCode {
    // Arrange: Select the unchanged seven-file SimpleDemo scenario.
    let name = "simple_demo";
    // Act: Run the real-ROM session and display its live commands and framebuffer.
    let result = visible::run(name);
    // Assert: Return the session/window failure or completed sequence's success.
    result
}
