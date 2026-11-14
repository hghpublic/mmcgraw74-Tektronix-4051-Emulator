//! Visible initial-input smoke regression for the supplied SVG2FG viewer.

#[path = "support/session.rs"]
mod session;
#[path = "support/visible.rs"]
mod visible;
#[path = "support/worker.rs"]
mod worker;

// What: Import SVG2FG and run file 1 to its single/all-files selection prompt.
// Why: Cover viewer startup separately from rendering the complete SVG collection.
fn main() -> std::process::ExitCode {
    // Arrange: Select the unchanged SVG2FG set, preserving duplicate-number replacement order.
    let name = "svg2fg";
    // Act: Display boot/FIND/OLD/RUN and the initial-input smoke outcome.
    let result = visible::run(name);
    // Assert: Keep missing readiness or unsupported execution as a nonzero failure.
    result
}
