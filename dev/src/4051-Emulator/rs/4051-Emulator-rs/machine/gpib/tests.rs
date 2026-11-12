use super::rtc_timestamp;
use jiff::civil::DateTime;

// What: Format fixed local dates across month, year, and millisecond boundaries.
// Why: The RTC must match the web's English month names and unpadded year remainder.
#[test]
fn rtc_formats_reference_date_and_time() {
    // Arrange: Select dates that exercise single-digit year, leap day, and December.
    let cases = [
        (
            DateTime::new(2001, 1, 2, 3, 4, 5, 6_999_999).unwrap(),
            "02-JAN-1 03:04:05:006",
        ),
        (
            DateTime::new(2024, 2, 29, 23, 59, 59, 999_999_999).unwrap(),
            "29-FEB-24 23:59:59:999",
        ),
        (
            DateTime::new(2100, 12, 31, 0, 0, 0, 0).unwrap(),
            "31-DEC-0 00:00:00:000",
        ),
    ];

    // Act: Format each date using the RTC response encoder.
    for (date, expected) in cases {
        let response = rtc_timestamp(date);

        // Assert: Each byte sequence matches the reference's exact formatting.
        assert_eq!(response, expected.as_bytes());
    }
}
