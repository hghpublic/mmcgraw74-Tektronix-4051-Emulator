use crate::flash_drive::{FileType, FileUsage, parse_header};

use super::{numbered_source_lines, prepare_file, program_file_bytes};

// What: Accept numbered ASCII source with LF, CRLF, and legacy CR record separators.
// Why: Browser editing normalizes newlines while imported 4051 BASIC files may use CR records.
#[test]
fn numbered_source_lines_accepts_supported_record_separators() {
    // Arrange: Include a UTF-8 BOM, blank records, and all supported line endings.
    let source = "\u{feff}10 PAGE\r\n\r20 PRINT 2+3\n30 END";

    // Act: Prepare source lines for the shared BASIC input sequence.
    let lines = numbered_source_lines(source).expect("numbered ASCII source should be accepted");

    // Assert: Empty records are skipped and every executable line remains ordered.
    assert_eq!(lines, ["10 PAGE", "20 PRINT 2+3", "30 END"]);
}

// What: Reject empty, malformed, duplicate-number, and non-ASCII source.
// Why: Invalid editor text must not be sent as immediate BASIC input or silently altered.
#[test]
fn numbered_source_lines_rejects_invalid_program_records() {
    // Arrange: Each source violates a distinct editor input requirement.
    let invalid_sources = [
        "",
        "PRINT 1",
        "10 PAGE\n10 END",
        "10 PRINT \"é\"",
        "\t10 PAGE",
        "10 PAGE\t",
        "\u{00a0}10 PAGE",
    ];

    // Act: Validate each source independently.
    let errors: Vec<_> = invalid_sources
        .iter()
        .map(|source| numbered_source_lines(source).expect_err("invalid source should fail"))
        .collect();

    // Assert: Every invalid source produces an explicit error.
    assert!(errors.iter().all(|error| !error.is_empty()));
}

// What: Normalize accepted editor line endings into CR-terminated BASIC records.
// Why: The private tape path must use the same record format as saved BASIC files.
#[test]
fn program_file_bytes_normalizes_record_separators() {
    // Arrange: Mix accepted line endings and a blank line in valid numbered source.
    let source = "10 PAGE\r\n\r20 END\n";

    // Act: Convert the source to the private tape's file payload.
    let bytes = program_file_bytes(source).expect("valid source should become a BASIC file");

    // Assert: Each retained BASIC line ends in exactly one CR byte.
    assert_eq!(bytes, b"10 PAGE\r20 END\r");
}

// What: Build a sanitized, numbered host filename and CR-terminated BASIC payload.
// Why: Saved editor output must import through the existing Flash Drive filename parser.
#[test]
fn prepared_file_is_a_flash_drive_importable_bas_program() {
    // Arrange: The lowest free file number is 2, and the requested title contains unsafe characters.
    let prepared = prepare_file("../Editor: Demo.bas", "10 PAGE\n20 END\n", [1, 3])
        .expect("valid numbered source should produce a host file");

    // Act: Parse the generated filename with the same metadata parser used for Flash Drive import.
    let header = parse_header(&prepared.name.encode_utf16().collect::<Vec<_>>())
        .expect("the generated filename should contain a valid Flash Drive header");

    // Assert: The filename and bytes describe an ASCII BASIC program importable as file 2.
    assert!(prepared.name.ends_with(".bas"));
    assert!(
        !prepared
            .name
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0' | ':' | '*' | '?'))
    );
    assert_eq!(header.number, Some(2));
    assert_eq!(header.file_type, FileType::Ascii);
    assert_eq!(header.usage, Some(FileUsage::Program));
    assert_eq!(
        String::from_utf16(&header.description).expect("the description should be valid UTF-16"),
        ".._Editor_ Demo"
    );
    assert_eq!(prepared.bytes, b"10 PAGE\r20 END\r");
}

// What: Report exhausted Flash Drive numbering and blank editor filenames.
// Why: Save must not overwrite an existing file or create an unusable import name implicitly.
#[test]
fn prepared_file_rejects_missing_name_or_full_directory() {
    // Arrange: Source is valid, while the two cases have no usable destination metadata.
    let source = "10 END";
    let full_directory = 1..=254;

    // Act: Request a blank title and then a title when every numbered slot is occupied.
    let blank_name = prepare_file("  ", source, []);
    let no_file_number = prepare_file("Program", source, full_directory);

    // Assert: Both failure conditions are returned explicitly.
    assert!(blank_name.is_err());
    assert!(no_file_number.is_err());
}
