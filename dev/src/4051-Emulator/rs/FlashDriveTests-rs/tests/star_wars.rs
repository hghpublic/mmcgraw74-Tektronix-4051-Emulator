//! Visible startup smoke regression for the unchanged StarWars Flash Drive set.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import StarWars and run its file-1 Tie Fighter drawing program.
// Why: Cover initial drawing and command readiness, not the other supplied game variants.
fn main() -> std::process::ExitCode {
    // Arrange: Select the supplied StarWars directory and file 1.
    let name = "star_wars";
    // Act: Display the real-ROM load/run sequence and changed drawing.
    let result = visible::run(name);
    // Assert: Return success only for the configured smoke checks, or the explicit failure.
    result
}
