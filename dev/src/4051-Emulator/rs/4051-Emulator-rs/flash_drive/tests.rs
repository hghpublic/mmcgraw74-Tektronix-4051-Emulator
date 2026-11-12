use super::{
    FileType, FileUsage, FlashDriveExportError, encode_archive_header, encode_host_filename,
    metadata_units, parse_archive_header, parse_header,
};

// What: Encode a Unicode filename and parse fixed-width metadata with reference marker rules.
// Why: Host filenames and LIST must share formatting without tightening legacy header interpretation.
#[test]
fn filenames_preserve_unicode_and_reference_metadata() {
    // Arrange: A supplementary Unicode character consumes two of the 16 description units.
    let description: Vec<_> = "\u{394}\u{1f600}".encode_utf16().collect();
    let expected = concat!(
        "7      BINARY  DATA ",
        "\u{394}\u{1f600}",
        "             ",
        " 3"
    );

    // Act: Encode the host name, derive LIST units and decode the formatted metadata.
    let name = encode_host_filename(
        7,
        Some(7),
        Some(FileType::Binary),
        Some(FileUsage::Data),
        &description,
        3,
    )
    .unwrap();
    let units = metadata_units(
        Some(7),
        Some(FileType::Binary),
        Some(FileUsage::Data),
        &description,
        3,
    )
    .unwrap();
    let header = parse_header(&units).unwrap();

    // Assert: Exact name, shared LIST units and padded raw UTF-16 metadata agree.
    assert_eq!(name, expected);
    assert_eq!(units, expected.encode_utf16().collect::<Vec<_>>());
    assert_eq!(header.number, Some(7));
    assert_eq!(header.file_type, FileType::Binary);
    assert_eq!(header.usage, Some(FileUsage::Data));
    let mut padded = description;
    padded.resize(16, 0x20);
    assert_eq!(header.description, padded);

    // Arrange: An out-of-range NEW header includes usage text, brackets and a nonnumeric size.
    let marker: Vec<_> = concat!("255    ", "NEW     ", "LOG  ", "[marker] ", "not-a-size")
        .encode_utf16()
        .collect();

    // Act: Decode without applying the machine's retained-number/usage state policy.
    let header = parse_header(&marker).unwrap();

    // Assert: Parsing retains the marker, ignores usage/size and removes description brackets.
    assert_eq!(header.number, None);
    assert_eq!(header.file_type, FileType::New);
    assert_eq!(header.usage, None);
    assert_eq!(
        header.description,
        "marker          ".encode_utf16().collect::<Vec<_>>()
    );
}

// What: Reject unsafe or unrepresentable host filenames while keeping raw LIST metadata available.
// Why: Host path safety must not alter machine-visible UTF-16 data or silently fabricate a name.
#[test]
fn filenames_reject_unsafe_metadata_without_changing_list_units() {
    // Arrange: Separators, NUL and an isolated surrogate are valid raw description units.
    let descriptions = [vec![0x2f], vec![0x5c], vec![0], vec![0xd800]];

    // Act: Format LIST units and independently attempt host filename encoding.
    for description in descriptions {
        let units = metadata_units(
            Some(7),
            Some(FileType::Ascii),
            Some(FileUsage::Program),
            &description,
            0,
        )
        .unwrap();
        let error = encode_host_filename(
            7,
            Some(7),
            Some(FileType::Ascii),
            Some(FileUsage::Program),
            &description,
            0,
        )
        .unwrap_err();

        // Assert: Raw units remain at the description position, but host export fails explicitly.
        assert_eq!(&units[20..20 + description.len()], description);
        assert_eq!(error, FlashDriveExportError::InvalidFilename(7));
    }

    // Arrange: A 17-unit description exceeds the shared fixed-width metadata limit.
    let description = [0x41; 17];

    // Act: Attempt host encoding with otherwise valid metadata.
    let error = encode_host_filename(
        7,
        Some(7),
        Some(FileType::Ascii),
        Some(FileUsage::Program),
        &description,
        0,
    )
    .unwrap_err();

    // Assert: The original invalid-filename error identifies the requested file.
    assert_eq!(error, FlashDriveExportError::InvalidFilename(7));
}

// What: Decode/encode boundary file numbers, optional usage and a 16-unit UTF-16 description.
// Why: Archive metadata must retain its independent type/usage and lossless description rules.
#[test]
fn archive_headers_preserve_boundary_metadata() {
    // Arrange: Author the maximum-number header with an isolated surrogate at the description limit.
    let source = br#"[254,"N","T","12345678901234\uD800x"]"#;
    let mut description: Vec<u16> = "12345678901234".encode_utf16().collect();
    description.extend([0xd800, 0x78]);

    // Act: Decode the header and encode its borrowed metadata without a tape or ZIP stream.
    let header = parse_archive_header(source).unwrap();
    let encoded = encode_archive_header(
        header.number,
        Some(header.file_type),
        header.usage,
        &header.description,
    )
    .unwrap();

    // Assert: NEW plus TEXT remains valid, and exactly 16 raw units survive canonical escaping.
    assert_eq!(header.number, 254);
    assert_eq!(header.file_type, FileType::New);
    assert_eq!(header.usage, Some(FileUsage::Text));
    assert_eq!(header.description, description);
    assert_eq!(encoded, r#"[254,"N","T","12345678901234\ud800x"]"#);

    // Arrange: The minimum file number has an empty usage even with an ASCII type.
    let source = br#"[1,"A","","short"]"#;

    // Act: Decode and encode that independently authored header.
    let header = parse_archive_header(source).unwrap();
    let encoded = encode_archive_header(
        header.number,
        Some(header.file_type),
        header.usage,
        &header.description,
    )
    .unwrap();

    // Assert: Archive usage is not forced by the stricter machine LIST selection rules.
    assert_eq!(header.number, 1);
    assert_eq!(header.file_type, FileType::Ascii);
    assert_eq!(header.usage, None);
    assert_eq!(encoded.as_bytes(), source);
}

// What: Reject invalid archive metadata and missing export type with explicit errors.
// Why: Extracting the codec must not weaken validation or synthesize a success-shaped header.
#[test]
fn archive_headers_report_invalid_metadata() {
    // Arrange: Exercise number bounds/grammar, unsupported codes, field width and invalid UTF-8.
    let invalid: &[(&[u8], &str)] = &[
        (br#"[0,"A","P","x"]"#, "outside 1..=254"),
        (br#"[255,"A","P","x"]"#, "outside 1..=254"),
        (br#"[01,"A","P","x"]"#, "invalid file number"),
        (br#"[1,"","P","x"]"#, "type must be one file-type code"),
        (br#"[1,"AB","P","x"]"#, "type must be one file-type code"),
        (br#"[1,"Z","P","x"]"#, "unsupported file type"),
        (
            br#"[1,"A","PD","x"]"#,
            "usage must be empty or one usage code",
        ),
        (br#"[1,"A","Q","x"]"#, "unsupported usage code"),
        (
            br#"[1,"A","P","12345678901234567"]"#,
            "exceeds 16 UTF-16 units",
        ),
        (&[0xff], "not UTF-8 JSON"),
    ];

    // Act and Assert: Every invalid header fails with its caller-visible diagnostic.
    for &(source, diagnostic) in invalid {
        let error = parse_archive_header(source).unwrap_err();
        assert!(error.contains(diagnostic), "{source:?}: {error}");
    }

    // Arrange: An untyped stored entry has no code that can be exported.
    let file_type = None;

    // Act: Attempt encoding instead of inventing a type.
    let error = encode_archive_header(7, file_type, None, &[]).unwrap_err();

    // Assert: The original missing-type export diagnostic is retained exactly.
    assert_eq!(error, "Flash Drive file 7 has no type");
}
