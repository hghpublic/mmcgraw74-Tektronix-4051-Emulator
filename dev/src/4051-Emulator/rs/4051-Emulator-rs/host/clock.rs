//! Default host-local calendar sampling for the machine's RTC callback.
//!
//! Sampling occurs on demand, not on a frame timer. Native builds use the
//! system clock; browser builds convert JavaScript epoch milliseconds. GPIB
//! owns response formatting and propagates these failures through the bus.

use jiff::{Timestamp, civil::DateTime, tz::TimeZone};

#[cfg(not(target_arch = "wasm32"))]
use std::time::SystemTime;

/// Converts the browser's millisecond wall clock with checked timestamp bounds.
#[cfg(target_arch = "wasm32")]
fn timestamp_now() -> Result<Timestamp, jiff::Error> {
    let milliseconds = js_sys::Date::now();
    if !milliseconds.is_finite() {
        return Err(jiff::Error::from_args(format_args!(
            "browser clock returned a non-finite timestamp"
        )));
    }
    let milliseconds = i64::try_from(milliseconds as i128).map_err(|_| {
        jiff::Error::from_args(format_args!("browser clock timestamp is out of range"))
    })?;
    let nanosecond = i32::try_from(milliseconds.rem_euclid(1_000) * 1_000_000).map_err(|_| {
        jiff::Error::from_args(format_args!(
            "browser clock subsecond value is out of range"
        ))
    })?;
    Timestamp::new(milliseconds.div_euclid(1_000), nanosecond)
}

#[cfg(not(target_arch = "wasm32"))]
fn timestamp_now() -> Result<Timestamp, jiff::Error> {
    Timestamp::try_from(SystemTime::now())
}

/// Samples the platform clock and resolves it in the configured local timezone.
///
/// Clock-range and timezone failures are returned; UTC is never substituted.
pub fn local_datetime() -> Result<DateTime, jiff::Error> {
    let zone = TimeZone::try_system()?;
    Ok(zone.to_datetime(timestamp_now()?))
}
