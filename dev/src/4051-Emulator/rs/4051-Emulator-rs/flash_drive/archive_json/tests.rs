use super::{ArchiveHeaderParser, json_utf16_string};

// What: Encode and decode control characters, Unicode pairs and isolated UTF-16 surrogates.
// Why: Archive descriptions must survive JSON interchange even when they are not valid Rust strings.
#[test]
fn archive_strings_preserve_exact_utf16_units() {
    // Arrange: Include quote/slash escapes, every shorthand control, NUL and paired/isolated surrogates.
    let units = [
        0x22, 0x5c, 0, 0x08, 0x0c, 0x0a, 0x0d, 0x09, 0x2f, 0xd83d, 0xde00, 0xd800, 0x41, 0xdc00,
    ];
    let source = r#""\"\\\u0000\b\f\n\r\t\/\uD83D\uDE00\uD800A\uDC00" "#;
    let mut parser = ArchiveHeaderParser::new(source);

    // Act: Encode the units; decode both escaped pairs and the encoder's raw Unicode scalar.
    let encoded = json_utf16_string(&units);
    let decoded = parser.parse_string().unwrap();
    parser.finish().unwrap();
    let decoded_scalar = ArchiveHeaderParser::new(&encoded).parse_string().unwrap();

    // Assert: Exact escaping/scalar output and raw decoded units both preserve the description.
    assert_eq!(
        encoded,
        "\"\\\"\\\\\\u0000\\b\\f\\n\\r\\t/\u{1f600}\\ud800A\\udc00\""
    );
    assert_eq!(decoded, units);
    assert_eq!(decoded_scalar, units);
}

// What: Reject malformed JSON string escapes, raw controls and trailing data.
// Why: Invalid archive syntax must remain an explicit error before storage replacement can commit.
#[test]
fn archive_strings_reject_invalid_syntax_and_trailing_data() {
    // Arrange: Author invalid strings for each escape/control boundary rather than using the encoder.
    let invalid = [
        (r#""\u12""#, "incomplete Unicode escape"),
        (r#""\u12g4""#, "invalid Unicode escape"),
        (r#""\q""#, "invalid JSON escape"),
        ("\"raw\0control\"", "unescaped JSON control character"),
        ("\"unfinished", "unterminated JSON string"),
    ];

    // Act and Assert: Each parse fails with the corresponding explicit diagnostic.
    for (source, diagnostic) in invalid {
        let error = ArchiveHeaderParser::new(source).parse_string().unwrap_err();
        assert!(error.contains(diagnostic), "{source:?}: {error}");
    }

    // Arrange: A valid string followed by another value is not one complete header field.
    let mut parser = ArchiveHeaderParser::new(r#""ok" true"#);

    // Act: Read the valid prefix, then require the JSON input to be exhausted.
    assert_eq!(parser.parse_string().unwrap(), [0x6f, 0x6b]);
    let error = parser.finish().unwrap_err();

    // Assert: Trailing input is rejected explicitly rather than silently ignored.
    assert!(error.contains("trailing JSON data"), "{error}");
}
