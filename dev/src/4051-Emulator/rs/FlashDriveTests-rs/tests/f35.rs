//! Visible smoke regression for F35's supplied file-200 drawing program.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import F35 and run the user-selected BMP DISPLAY program in file 200.
// Why: File 1 contains binary drawing data and must not be treated as a BASIC entry.
fn main() -> std::process::ExitCode {
    // Arrange: Select the unchanged F35 set and its file-200 load sequence.
    let name = "f35";
    // Act: Display boot/FIND/OLD/RUN and wait for changed drawing and command readiness.
    let result = visible::run(name);
    // Assert: Preserve completion success or a nonzero drawing/execution failure.
    result
}
