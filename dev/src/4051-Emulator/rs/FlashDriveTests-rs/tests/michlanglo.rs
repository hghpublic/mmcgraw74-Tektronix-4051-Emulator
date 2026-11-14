//! Visible initial-input smoke regression for the supplied Michlanglo directory.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Michlanglo and run file 1 to its filled/unfilled polygon prompt.
// Why: Exercise original startup without claiming polygon rendering or R12 file creation.
fn main() -> std::process::ExitCode {
    // Arrange: Preserve the supplied directory spelling and file-1 drawing program.
    let name = "michlanglo";
    // Act: Display the shared startup sequence and initial-input smoke result.
    let result = visible::run(name);
    // Assert: Return the real startup outcome without changing supplied data or mode choices.
    result
}
