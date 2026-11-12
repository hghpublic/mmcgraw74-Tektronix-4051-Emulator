//! Native executable entry point; session execution belongs to the library.
//! Native regressions compile the same adapter source inside private session tests.

#[cfg(not(test))]
#[path = "native/app.rs"]
mod native;

#[cfg(not(test))]
fn main() -> eframe::Result {
    native::run()
}
