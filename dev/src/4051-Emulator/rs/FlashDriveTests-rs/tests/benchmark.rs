//! Visible startup smoke regression for the unchanged Benchmark Flash Drive set.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import Benchmark and run file 1 through its initial input screen.
// Why: Verify startup separately from benchmark choices, completion and timing scores.
fn main() -> std::process::ExitCode {
    // Arrange: Select the supplied Benchmark directory and file 1.
    let name = "benchmark";
    // Act: Display boot/FIND/OLD/RUN and wait for the input-screen smoke predicate.
    let result = visible::run(name);
    // Assert: Propagate the real session outcome rather than skipping unsupported behavior.
    result
}
