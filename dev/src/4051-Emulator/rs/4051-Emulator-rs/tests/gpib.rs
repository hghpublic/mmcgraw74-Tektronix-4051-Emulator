use std::{
    fs::File,
    io::{Cursor, Read, Write},
    process::Command,
    time::SystemTime,
};
use tek4051_emulator::machine::{
    Machine, MachineBusError,
    gpib::{
        FlashDriveExportError, FlashDriveFile, FlashDriveFileInfo, FlashDriveImportError,
        GpibBuffers, Loader, MAX_FLASH_ARCHIVE_ENTRIES, MAX_FLASH_ARCHIVE_ENTRY_BYTES,
        MAX_FLASH_ARCHIVE_FILE_BYTES, MAX_FLASH_ARCHIVE_TOTAL_BYTES,
    },
};

use jiff::{Timestamp, civil::DateTime, tz::TimeZone};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn configure_gpib_talker(machine: &mut Machine) {
    machine.write_byte(0x87b0, 0xff).unwrap();
    machine.write_byte(0x87b2, 0x0f).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b3, 0x3c).unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
}

// What: List canonical metadata in numeric order, retaining markers and raw UTF-16 descriptions.
// Why: A read-only host directory must show every stored file without copying or decoding its payload.
#[test]
fn flash_directory_lists_sorted_canonical_metadata_and_raw_descriptions() {
    // Arrange: An empty store has no entries; import shuffled files with all types and usages.
    let mut machine = Machine::default();
    assert!(machine.flash_drive_directory().is_empty());
    let files = [
        ("254    LAST         end 999", vec![]),
        ("4      ASCII   TEXT note 999", b"note\r".to_vec()),
        ("1      BINARY  DATA \u{0394}\u{1f600} 999", vec![0, 0xff]),
        ("7      NEW          new 999", vec![9]),
        ("3      ASCII   PROG abcdefghijklmno\u{1f600} 999", vec![1]),
        ("6      BINARY  LOG  log 999", vec![3, 4, 5]),
    ]
    .map(|(name, bytes)| FlashDriveFile {
        name: name.to_owned(),
        bytes,
    });
    assert_eq!(machine.import_flash_files(&files), vec![Ok(()); 6]);
    let padded = |text: &str| {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.resize(16, u16::from(b' '));
        units
    };
    let mut unicode = vec![0x0394, 0xd83d, 0xde00];
    unicode.resize(16, u16::from(b' '));
    let mut split_surrogate: Vec<u16> = b"abcdefghijklmno".iter().copied().map(u16::from).collect();
    split_surrogate.push(0xd83d);

    // Act: Obtain a metadata-only snapshot without FIND or an export operation.
    let directory = machine.flash_drive_directory();

    // Assert: Numbers, labels, raw descriptions and real byte sizes match independent expectations.
    assert_eq!(
        directory,
        [
            FlashDriveFileInfo {
                number: 1,
                file_type: Some("BINARY"),
                usage: Some("DATA"),
                description: unicode,
                byte_len: 2
            },
            FlashDriveFileInfo {
                number: 3,
                file_type: Some("ASCII"),
                usage: Some("PROG"),
                description: split_surrogate,
                byte_len: 1
            },
            FlashDriveFileInfo {
                number: 4,
                file_type: Some("ASCII"),
                usage: Some("TEXT"),
                description: padded("note"),
                byte_len: 5
            },
            FlashDriveFileInfo {
                number: 6,
                file_type: Some("BINARY"),
                usage: Some("LOG"),
                description: padded("log"),
                byte_len: 3
            },
            FlashDriveFileInfo {
                number: 7,
                file_type: Some("NEW"),
                usage: None,
                description: padded("new"),
                byte_len: 1
            },
            FlashDriveFileInfo {
                number: 254,
                file_type: Some("LAST"),
                usage: None,
                description: padded("end"),
                byte_len: 0
            },
        ]
    );
}

// What: List after SAVE and before WRITE without changing the selected snapshot, cursor or clear flag.
// Why: The directory must report canonical updates while leaving subsequent GPIB transfers unchanged.
#[test]
fn flash_directory_reads_saved_store_without_disturbing_transfer_state() {
    // Arrange: Consume a record from file 1 and retain an independent binary data file.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\rsecond\r").unwrap();
    assert_eq!(
        machine.import_flash_files(&[FlashDriveFile {
            name: "2      BINARY  DATA bytes 999".to_owned(),
            bytes: vec![11, 22, 33],
        }]),
        [Ok(())]
    );
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"old\r");

    // Act: Snapshot metadata, then SAVE to the retained selection and read a fresh directory.
    let before = machine.flash_drive_directory();
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x61, b"10 END\r").unwrap();
    let after = machine.flash_drive_directory();

    // Assert: SAVE still targets file 1; metadata is canonical while OLD/INPUT retain the old snapshot.
    assert_eq!(before[0].byte_len, 11);
    assert_eq!(after[0].byte_len, 7);
    assert_eq!(after[1].byte_len, 3);
    assert_eq!(tape_read(&mut machine, 0x64), b"old\rsecond\r");
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"second\r");

    // Act: FIND data arms WRITE clearing; querying metadata must leave it armed and selection unchanged.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    assert_eq!(machine.flash_drive_directory()[1].byte_len, 3);
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0xaa]).unwrap();

    // Assert: The first WRITE replaces old binary bytes, and both the store and directory see one byte.
    assert_eq!(machine.export_flash_file(2).unwrap().bytes, [0xaa]);
    assert_eq!(machine.flash_drive_directory()[1].byte_len, 1);
    assert_eq!(machine.export_flash_file(1).unwrap().bytes, b"10 END\r");
}

// What: Export canonical ASCII/binary files and markers, then import their numbered names again.
// Why: Host export must preserve raw bytes, Unicode metadata and real size rather than imported size labels.
#[test]
fn flash_export_round_trips_bytes_and_representable_metadata() {
    // Arrange: Include program/text/data/log usage, Unicode descriptions, markers and non-text bytes.
    for (number, name, bytes, expected_name) in [
        (
            1,
            "1      ASCII   PROG Program 999",
            b"10 PRINT 7\r\r".as_slice(),
            "1      ASCII   PROG Program          12",
        ),
        (
            2,
            "2      ASCII   TEXT Text 999",
            b"one\r".as_slice(),
            "2      ASCII   TEXT Text             4",
        ),
        (
            3,
            "3      BINARY  DATA \u{0394}\u{1f600} 999",
            &[0, 0xff, 0x0d, 0x7f],
            "3      BINARY  DATA \u{0394}\u{1f600}              4",
        ),
        (
            4,
            "4      BINARY  LOG  Log 999",
            &[0xff, 0, 1],
            "4      BINARY  LOG  Log              3",
        ),
        (
            5,
            "5      NEW          Marker 999",
            &[],
            "5      NEW          Marker           0",
        ),
        (
            254,
            "254    LAST         Last 999",
            &[0, 0xff],
            "254    LAST         Last             2",
        ),
    ] {
        let mut machine = Machine::default();
        assert_eq!(
            machine.import_flash_files(&[FlashDriveFile {
                name: name.to_owned(),
                bytes: bytes.to_vec(),
            }]),
            [Ok(())]
        );

        // Act: Export from the canonical store and import the generated filename into a fresh store.
        let exported = machine.export_flash_file(number).unwrap();
        let mut restored = Machine::default();
        assert_eq!(
            restored.import_flash_files(&[FlashDriveFile {
                name: exported.name.clone(),
                bytes: exported.bytes.clone(),
            }]),
            [Ok(())]
        );
        let round_trip = restored.export_flash_file(number).unwrap();

        // Assert: Bytes and canonical filename metadata survive; stale filename size is not retained.
        assert_eq!(exported.bytes, bytes);
        assert_eq!(exported.name, expected_name);
        assert_eq!(round_trip.name, exported.name);
        assert_eq!(round_trip.bytes, exported.bytes);
    }
}

// What: Stream a full store into the web's ordered JSON-header/content ZIP format.
// Why: Archive descriptions are data, not host paths, and must preserve quotes, NULs and raw bytes.
#[test]
fn flash_storage_archive_writes_web_compatible_ordered_pairs() {
    // Arrange: Import out-of-order ASCII, binary and LAST entries, including path/control characters.
    let mut machine = Machine::default();
    let files = [
        FlashDriveFile {
            name: "20     BINARY  DATA Payload 999".to_owned(),
            bytes: vec![0, 0xff],
        },
        FlashDriveFile {
            name: "1      ASCII   TEXT Quote\"/A\\B\0C 999".to_owned(),
            bytes: b"PRINT 1\r".to_vec(),
        },
        FlashDriveFile {
            name: "254    LAST         Marker 0".to_owned(),
            bytes: Vec::new(),
        },
    ];
    assert_eq!(machine.import_flash_files(&files), [Ok(()), Ok(()), Ok(())]);
    let directory = machine.flash_drive_directory();

    // Act: Write the archive to a seekable memory sink for format inspection.
    let archive_bytes = machine
        .write_flash_storage_archive(Cursor::new(Vec::new()))
        .unwrap()
        .into_inner();
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
    assert_eq!(archive.len(), 6);
    let expected = [
        (
            "0001",
            r#"[1,"A","T","Quote\"/A\\B\u0000C    "]"#.as_bytes(),
        ),
        ("0002", b"PRINT 1\r"),
        ("0003", r#"[20,"B","D","Payload         "]"#.as_bytes()),
        ("0004", &[0, 0xff]),
        ("0005", r#"[254,"L","","Marker          "]"#.as_bytes()),
        ("0006", b""),
    ];
    for (index, (name, contents)) in expected.into_iter().enumerate() {
        let mut entry = archive.by_index(index).unwrap();
        assert_eq!(entry.name(), name);
        let mut actual = Vec::new();
        entry.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, contents);
    }

    // Assert: Numeric ordering, metadata/content pairing and storage contents remain unchanged.
    assert_eq!(machine.flash_drive_directory(), directory);
}

// What: Reject whole-storage archive creation when the Flash Drive is empty.
// Why: The native UI should report an explicit failure instead of saving an empty replacement archive.
#[test]
fn flash_storage_archive_reports_empty_store() {
    // Arrange: A default machine contains no stored Flash Drive files.
    let machine = Machine::default();

    // Act: Attempt to write the archive to an in-memory sink.
    let error = machine
        .write_flash_storage_archive(Cursor::new(Vec::new()))
        .unwrap_err();

    // Assert: The API reports why no archive was produced.
    assert_eq!(error, "Flash Drive contains no files");
}

// What: Replace storage from header/content pairs with escapes, duplicate numbers, and a lone surrogate.
// Why: Archive import must preserve web metadata while invalidating snapshots for removed files.
#[test]
fn flash_storage_archive_import_replaces_store_and_resets_transfer_snapshot() {
    // Arrange: Consume an old snapshot, then build out-of-order pairs with a later duplicate.
    let mut machine = Machine::default();
    machine.store_tape_program(9, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"old\r");
    let archive_bytes = create_flash_storage_archive(&[
        ("any-name", r#"[2,"A","P","Second"]"#.as_bytes()),
        ("any-content-name", b"second\r"),
        (
            "header",
            r#"[1,"B","D","Quote\"/\u0000\\\ud83d"]"#.as_bytes(),
        ),
        ("content", &[0, 0xff]),
        ("unicode-header", r#"[3,"A","P","Δ"]"#.as_bytes()),
        ("unicode-content", b"delta\r"),
        ("duplicate-header", r#"[2,"A","T","Last"]"#.as_bytes()),
        ("duplicate-content", b"last\r"),
        ("marker-header", r#"[254,"L","P","Marker"]"#.as_bytes()),
        ("marker-content", b""),
    ]);

    // Act: Import the archive without relying on entry names or Rust String surrogate handling.
    machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap();

    // Assert: Sorted files retain escaped UTF-16 units, and the final duplicate replaces number 2.
    let mut escaped_description: Vec<_> = "Quote\"/".encode_utf16().collect();
    escaped_description.extend([0, u16::from(b'\\'), 0xd83d]);
    assert_eq!(
        machine.flash_drive_directory(),
        [
            FlashDriveFileInfo {
                number: 1,
                file_type: Some("BINARY"),
                usage: Some("DATA"),
                description: escaped_description,
                byte_len: 2,
            },
            FlashDriveFileInfo {
                number: 2,
                file_type: Some("ASCII"),
                usage: Some("TEXT"),
                description: "Last".encode_utf16().collect(),
                byte_len: 5,
            },
            FlashDriveFileInfo {
                number: 3,
                file_type: Some("ASCII"),
                usage: Some("PROG"),
                description: "Δ".encode_utf16().collect(),
                byte_len: 6,
            },
            FlashDriveFileInfo {
                number: 254,
                file_type: Some("LAST"),
                usage: Some("PROG"),
                description: "Marker".encode_utf16().collect(),
                byte_len: 0,
            },
        ]
    );
    assert_eq!(machine.export_flash_file(2).unwrap().bytes, b"last\r");
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x64), [0xff]);

    // Act: FIND the first replacement and request its OLD snapshot.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: The selected snapshot contains the imported raw binary bytes.
    assert_eq!(tape_read(&mut machine, 0x64), [0, 0xff]);

    // Act: FIND the later duplicate number and request its OLD snapshot.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();

    // Assert: The later pair replaced the earlier pair at file number 2.
    assert_eq!(tape_read(&mut machine, 0x64), b"last\r");
}

// What: Abort an OLD transfer when replacement occurs after bytes are queued but before PIA reads.
// Why: No response bytes from the replaced store may leak into the active tape transfer.
#[test]
fn flash_storage_archive_import_aborts_pending_old_response() {
    // Arrange: Select old bytes and queue an OLD response without polling its first byte.
    let mut machine = Machine::default();
    machine.store_tape_program(9, b"old response\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x64);
    configure_gpib_listener(&mut machine);
    let archive_bytes = create_flash_storage_archive_file(1, b"new response\r");

    // Act: Replace the store while the old response remains queued for the reader.
    machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap();

    // Assert: The active read receives only the empty-input sentinel with EOI.
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0xff);
    assert_ne!(machine.read_byte(0x87aa).unwrap() & 0x10, 0);
}

// What: Replace storage after OLD queues bytes but before the reader is configured.
// Why: The replacement sentinel must still complete the handshake once the listener comes online.
#[test]
fn flash_storage_archive_import_finishes_queued_read_before_listener_setup() {
    // Arrange: Dispatch OLD while the controller is still the talker, leaving its response queued.
    let mut machine = Machine::default();
    machine.store_tape_program(9, b"old response\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x64);
    let archive_bytes = create_flash_storage_archive_file(1, b"new response\r");

    // Act: Import before enabling the reader, then configure it and poll the response.
    machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap();
    configure_gpib_listener(&mut machine);

    // Assert: The queued old bytes were replaced by the sentinel and the read reaches EOI.
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0xff);
    assert_ne!(machine.read_byte(0x87aa).unwrap() & 0x10, 0);

    // Act: Acknowledge the aborted response, then address the replacement file with a new transaction.
    machine.write_byte(0x87b2, 0x98).unwrap();
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: The next OLD transfer completes with bytes from the replacement archive.
    assert_eq!(tape_read(&mut machine, 0x64), b"new response\r");
}

// What: Drop a partial SAVE when replacement occurs before its EOI byte.
// Why: Remaining bytes from an old transaction must not modify the new lowest-numbered file.
#[test]
fn flash_storage_archive_import_aborts_pending_save_output() {
    // Arrange: Accumulate a non-final SAVE byte for the currently addressed tape device.
    let mut machine = Machine::default();
    machine.store_tape_program(9, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    resume_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x25);
    send_gpib_command(&mut machine, 0x61);
    machine.write_byte(0x87b2, 0x08).unwrap();
    machine.write_byte(0x87aa, 0).unwrap();
    machine.write_byte(0x87b0, b'x').unwrap();
    machine.write_byte(0x87b1, 0x3c).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
    let archive_bytes = create_flash_storage_archive_file(1, b"replacement\r");

    // Act: Replace storage, then finish the old SAVE command with additional bytes and EOI.
    machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap();
    for (index, byte) in b"tail\r".iter().copied().enumerate() {
        machine
            .write_byte(
                0x87aa,
                if index + 1 == b"tail\r".len() {
                    0x10
                } else {
                    0
                },
            )
            .unwrap();
        machine.write_byte(0x87b0, byte).unwrap();
        machine.write_byte(0x87b1, 0x3c).unwrap();
        machine.write_byte(0x87b1, 0x34).unwrap();
    }

    // Assert: The archive's replacement bytes remain untouched by the abandoned SAVE.
    assert_eq!(
        machine.export_flash_file(1).unwrap().bytes,
        b"replacement\r"
    );
}

// What: Replace storage after FIND has advanced the cursor and armed the data-file WRITE clear.
// Why: New-store PRINT must start from zero, and an old clear must not erase the replacement.
#[test]
fn flash_storage_archive_import_resets_cursor_and_pending_write_clear() {
    // Arrange: FIND and INPUT one record from a data file, leaving both states active.
    let mut machine = Machine::default();
    assert_eq!(
        machine.import_flash_files(&[FlashDriveFile {
            name: "9      ASCII   DATA Old 999".to_owned(),
            bytes: b"first\rsecond\r".to_vec(),
        }]),
        [Ok(())]
    );
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"first\r");
    let archive_bytes = create_flash_storage_archive(&[
        ("header", r#"[1,"A","D","New"]"#.as_bytes()),
        ("content", b"base\r"),
    ]);

    // Act: Import a data file, PRINT to it, issue the rejected ASCII WRITE, then PRINT again.
    machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap();
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6c, b"prefix\r").unwrap();
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6f, &[0xaa]).unwrap();
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6c, b"suffix\r").unwrap();

    // Assert: The first PRINT replaced imported bytes and survived the old pending WRITE clear.
    assert_eq!(
        machine.export_flash_file(1).unwrap().bytes,
        b"prefix\rsuffix\r"
    );
}

// What: SAVE immediately after replacement without issuing another FIND.
// Why: The replacement store must select its lowest stored number for SAVE/BSAVE.
#[test]
fn flash_storage_archive_import_selects_lowest_number_for_save() {
    // Arrange: Import files 20 and 3 in reverse order, then configure the tape talker.
    let archive_bytes = create_flash_storage_archive(&[
        ("header-20", r#"[20,"A","P","Twenty"]"#.as_bytes()),
        ("content-20", b"old twenty\r"),
        ("header-3", r#"[3,"A","P","Three"]"#.as_bytes()),
        ("content-3", b"old three\r"),
    ]);
    let mut machine = Machine::default();
    machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap();
    configure_gpib_talker(&mut machine);

    // Act: SAVE without FIND, so the existing selection must come from the replacement.
    tape_write(&mut machine, 0x61, b"selected\r").unwrap();

    // Assert: File 3 is updated while file 20 retains its original size.
    let directory = machine.flash_drive_directory();
    assert_eq!(directory[0].number, 3);
    assert_eq!(directory[0].byte_len, b"selected\r".len());
    assert_eq!(directory[1].number, 20);
    assert_eq!(directory[1].byte_len, b"old twenty\r".len());
}

// What: Reject incomplete pairs and a later malformed JSON header after a valid pair.
// Why: A replacement archive must be fully validated before any current file or snapshot is lost.
#[test]
fn flash_storage_archive_import_is_atomic_on_malformed_input() {
    // Arrange: Select an existing file and prepare one odd-entry and one invalid-header archive.
    let mut machine = Machine::default();
    machine.store_tape_program(9, b"retained\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    let before = machine.flash_drive_directory();
    let odd_entries = create_flash_storage_archive(&[("orphan", b"[1,\"A\",\"P\",\"New\"]")]);
    let malformed_header = create_flash_storage_archive(&[
        ("valid-header", r#"[2,"A","P","New"]"#.as_bytes()),
        ("valid-content", b"new\r"),
        ("invalid-header", b"[1,\"A\",\"P\",\"New\",true]"),
        ("invalid-content", b"ignored"),
    ]);

    // Act: Attempt each invalid archive while the old file remains selected.
    let odd_error = machine
        .import_flash_storage_archive(Cursor::new(odd_entries))
        .unwrap_err();
    let header_error = machine
        .import_flash_storage_archive(Cursor::new(malformed_header))
        .unwrap_err();

    // Assert: Both failures are explicit and preserve canonical storage and the OLD snapshot.
    assert!(odd_error.contains("header/content entry pairs"));
    assert!(header_error.contains("expected"));
    assert_eq!(machine.flash_drive_directory(), before);
    assert_eq!(tape_read(&mut machine, 0x64), b"retained\r");
}

// What: Accept an 8 MiB content entry and reject one byte more without partial replacement.
// Why: The per-entry expansion cap must include its exact boundary and bound decompression.
#[test]
fn flash_storage_archive_import_enforces_per_entry_limit() {
    // Arrange: Prepare zero-filled payloads at and just beyond the shared entry limit.
    let boundary = vec![0; usize::try_from(MAX_FLASH_ARCHIVE_ENTRY_BYTES).unwrap()];
    let oversized = vec![0; boundary.len() + 1];
    let mut machine = Machine::default();
    let boundary_archive = create_flash_storage_archive_file(1, &boundary);
    let oversized_archive = create_flash_storage_archive_file(2, &oversized);

    // Act: Import the boundary archive, then attempt the oversized archive.
    machine
        .import_flash_storage_archive(Cursor::new(boundary_archive))
        .unwrap();
    let before = machine.flash_drive_directory();
    let error = machine
        .import_flash_storage_archive(Cursor::new(oversized_archive))
        .unwrap_err();

    // Assert: The exact boundary succeeds, while the oversized archive fails atomically.
    assert_eq!(
        machine.export_flash_file(1).unwrap().bytes.len(),
        boundary.len()
    );
    assert!(error.contains("8 MiB uncompressed entry limit"));
    assert_eq!(machine.flash_drive_directory(), before);
}

// What: Accept the exact aggregate expansion limit and reject one additional byte.
// Why: Many individually valid entries must not bypass the total decompression budget.
#[test]
fn flash_storage_archive_import_enforces_total_expansion_limit() {
    // Arrange: Build eight valid headers and contents summing to the exact aggregate boundary.
    let headers: Vec<_> = (1..=8)
        .map(|number| (number, format!(r#"[{number},"B","D",""]"#)))
        .collect();
    let metadata_bytes: usize = headers.iter().map(|(_, header)| header.len()).sum();
    let entry_limit = usize::try_from(MAX_FLASH_ARCHIVE_ENTRY_BYTES).unwrap();
    let payload = vec![0; entry_limit];
    let build_archive = |extra_content_bytes: usize| {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (index, (number, header)) in headers.iter().enumerate() {
            archive
                .start_file(format!("header-{number}"), options)
                .unwrap();
            archive.write_all(header.as_bytes()).unwrap();
            archive
                .start_file(format!("content-{number}"), options)
                .unwrap();
            let content_size = if index + 1 == headers.len() {
                entry_limit - metadata_bytes + extra_content_bytes
            } else {
                entry_limit
            };
            archive.write_all(&payload[..content_size]).unwrap();
        }
        archive.finish().unwrap().into_inner()
    };
    let exact_archive = build_archive(0);
    let mut machine = Machine::default();

    // Act: Import the exact-limit archive, then attempt a one-byte-over replacement.
    machine
        .import_flash_storage_archive(Cursor::new(exact_archive))
        .unwrap();
    let exact_directory = machine.flash_drive_directory();
    let error = machine
        .import_flash_storage_archive(Cursor::new(build_archive(1)))
        .unwrap_err();

    // Assert: The exact boundary succeeds, and the over-limit attempt preserves that populated store.
    assert_eq!(
        exact_directory
            .iter()
            .map(|file| file.byte_len as u64)
            .sum::<u64>()
            + metadata_bytes as u64,
        MAX_FLASH_ARCHIVE_TOTAL_BYTES
    );
    assert!(error.contains(&format!(
        "{} MiB total uncompressed limit",
        MAX_FLASH_ARCHIVE_TOTAL_BYTES / (1024 * 1024)
    )));
    assert_eq!(machine.flash_drive_directory(), exact_directory);
}

// What: Reject a ZIP whose central directory exceeds the configured entry-count bound.
// Why: Entry metadata must be bounded before any existing file or header/content pair can be committed.
#[test]
fn flash_storage_archive_import_enforces_entry_count_limit() {
    // Arrange: Preserve one existing file and create one more empty ZIP entry than the public cap.
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..=MAX_FLASH_ARCHIVE_ENTRIES {
        archive
            .start_file(format!("entry-{index:04}"), SimpleFileOptions::default())
            .unwrap();
    }
    let archive_bytes = archive.finish().unwrap().into_inner();
    let mut machine = Machine::default();
    assert_eq!(
        machine.import_flash_files(&[FlashDriveFile {
            name: "7      ASCII   PROG Retained 999".to_owned(),
            bytes: b"retained\r".to_vec(),
        }]),
        [Ok(())]
    );
    let before = machine.flash_drive_directory();

    // Act: Attempt to replace storage from the over-limit archive.
    let error = machine
        .import_flash_storage_archive(Cursor::new(archive_bytes))
        .unwrap_err();

    // Assert: The count limit is reported before pair parsing and preserves existing bytes.
    assert!(error.contains("entry limit"));
    assert_eq!(machine.flash_drive_directory(), before);
    assert_eq!(machine.export_flash_file(7).unwrap().bytes, b"retained\r");
}

// What: Check the inclusive compressed-file boundary before ZIP parsing.
// Why: Size rejection must happen before parsing and preserve an existing store.
#[test]
fn flash_storage_archive_import_enforces_compressed_file_limit() {
    // Arrange: Create sparse files at and one byte above the configured compressed limit.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("archive.zip");
    File::create(&path)
        .unwrap()
        .set_len(MAX_FLASH_ARCHIVE_FILE_BYTES)
        .unwrap();
    let mut machine = Machine::default();
    assert_eq!(
        machine.import_flash_files(&[FlashDriveFile {
            name: "7      ASCII   PROG Retained 999".to_owned(),
            bytes: b"retained\r".to_vec(),
        }]),
        [Ok(())]
    );
    let before = machine.flash_drive_directory();

    // Act: The exact-size invalid ZIP reaches parsing; extending it crosses the size limit.
    let at_limit_error = machine
        .import_flash_storage_archive(File::open(&path).unwrap())
        .unwrap_err();
    File::create(&path)
        .unwrap()
        .set_len(MAX_FLASH_ARCHIVE_FILE_BYTES + 1)
        .unwrap();
    let over_limit_error = machine
        .import_flash_storage_archive(File::open(&path).unwrap())
        .unwrap_err();

    // Assert: Both failures are explicit and leave the populated store and bytes unchanged.
    assert!(at_limit_error.contains("invalid ZIP archive"));
    assert!(!at_limit_error.contains("compressed file limit"));
    assert!(over_limit_error.contains("128 MiB compressed file limit"));
    assert_eq!(machine.flash_drive_directory(), before);
    assert_eq!(machine.export_flash_file(7).unwrap().bytes, b"retained\r");
}

// What: Treat a valid zero-entry ZIP as an intentional replacement with an empty store.
// Why: The explicit replacement confirmation allows users to clear storage with an empty archive.
#[test]
fn flash_storage_archive_import_accepts_empty_archive_and_clears_old_snapshot() {
    // Arrange: Start with a selected file and create a valid ZIP containing no entries.
    let mut machine = Machine::default();
    machine.store_tape_program(9, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"9").unwrap();
    let empty_archive = create_flash_storage_archive(&[]);

    // Act: Import the empty archive.
    machine
        .import_flash_storage_archive(Cursor::new(empty_archive))
        .unwrap();

    // Assert: Storage is empty and OLD cannot expose bytes from the replaced file.
    assert!(machine.flash_drive_directory().is_empty());
    assert_eq!(tape_read(&mut machine, 0x64), [0xff]);
}

// What: Export a SAVE-updated entry while the current file and FIND snapshot still contain older bytes.
// Why: Export must read the persisted numbered store without rewinding INPUT or changing OLD state.
#[test]
fn flash_export_reads_saved_store_without_mutating_transfer_state() {
    // Arrange: Consume one record of a selected ASCII snapshot, then SAVE new canonical bytes.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\rsecond\r").unwrap();
    machine.store_tape_program(2, b"20 END\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"old\r");
    // Act/Assert: Export a different number before SAVE; its bytes must not replace the selected target.
    assert_eq!(machine.export_flash_file(2).unwrap().bytes, b"20 END\r");
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x61, b"10 PRINT 9\r").unwrap();

    // Act: Export the updated stored program without another FIND or any GPIB transaction.
    let export_name = machine.export_flash_file_name(1).unwrap();
    let exported = machine.export_flash_file(1).unwrap();

    // Assert: Canonical bytes are new, while OLD and the next INPUT record remain from the old snapshot.
    assert_eq!(exported.bytes, b"10 PRINT 9\r");
    assert_eq!(export_name, exported.name);
    assert_eq!(machine.export_flash_file(2).unwrap().bytes, b"20 END\r");
    assert_eq!(tape_read(&mut machine, 0x64), b"old\rsecond\r");
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"second\r");
}

// What: Export invalid/missing numbers and descriptions that cannot form faithful host filenames.
// Why: Never silently substitute a file or lose split UTF-16/path characters during native export.
#[test]
fn flash_export_rejects_missing_numbers_and_unrepresentable_names() {
    // Arrange: Start with an empty store and import accepted metadata containing host-name limitations.
    let mut machine = Machine::default();

    // Act/Assert: Invalid range and missing entries are distinct, explicit errors.
    for number in [0, 255, 7] {
        assert_eq!(
            machine.export_flash_file_name(number).unwrap_err(),
            machine.export_flash_file(number).unwrap_err()
        );
    }
    assert_eq!(
        machine.export_flash_file(0).unwrap_err(),
        FlashDriveExportError::FileNumberOutOfRange(0)
    );
    assert_eq!(
        machine.export_flash_file(255).unwrap_err(),
        FlashDriveExportError::FileNumberOutOfRange(255)
    );
    assert_eq!(
        machine.export_flash_file(7).unwrap_err(),
        FlashDriveExportError::FileNotFound(7)
    );

    // Act: Import a description split at 16 UTF-16 units, plus unsafe single-component names.
    for description in [
        "abcdefghijklmno\u{1f600}",
        "../bad",
        "bad\\name",
        "bad\0name",
    ] {
        assert_eq!(
            machine.import_flash_files(&[FlashDriveFile {
                name: format!("1      ASCII   PROG {description} 0"),
                bytes: vec![],
            }]),
            [Ok(())]
        );

        // Assert: Metadata stays accepted by import, but export refuses lossy or unsafe filenames.
        assert_eq!(
            machine.export_flash_file_name(1).unwrap_err(),
            FlashDriveExportError::InvalidFilename(1)
        );
        assert_eq!(
            machine.export_flash_file(1).unwrap_err(),
            FlashDriveExportError::InvalidFilename(1)
        );
    }
}

fn send_gpib_command(machine: &mut Machine, byte: u8) {
    machine.write_byte(0x87b0, byte).unwrap();
    machine.write_byte(0x87b1, 0x3c).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
}

fn configure_gpib_listener(machine: &mut Machine) {
    machine.write_byte(0x87b1, 0x30).unwrap();
    machine.write_byte(0x87b0, 0).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b3, 0x34).unwrap();
    machine.write_byte(0x87b2, 0x18).unwrap();
}

fn resume_gpib_talker(machine: &mut Machine) {
    machine.write_byte(0x87b1, 0x30).unwrap();
    machine.write_byte(0x87b0, 0xff).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b3, 0x3c).unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
}

fn tape_write(machine: &mut Machine, command: u8, bytes: &[u8]) -> Result<(), MachineBusError> {
    send_gpib_command(machine, 0x25);
    send_gpib_command(machine, command);
    machine.write_byte(0x87b2, 0x08).unwrap();
    for (index, &byte) in bytes.iter().enumerate() {
        machine
            .write_byte(0x87aa, if index + 1 == bytes.len() { 0x10 } else { 0 })
            .unwrap();
        machine.write_byte(0x87b0, byte).unwrap();
        machine.write_byte(0x87b1, 0x3c)?;
        machine.write_byte(0x87b1, 0x34).unwrap();
    }
    Ok(())
}

fn tape_find(machine: &mut Machine, number: &[u8]) -> Result<(), MachineBusError> {
    tape_write(machine, 0x7b, number)
}

fn tape_read(machine: &mut Machine, command: u8) -> Vec<u8> {
    machine.write_byte(0x87b2, 0x00).unwrap();
    send_gpib_command(machine, 0x45);
    send_gpib_command(machine, command);
    configure_gpib_listener(machine);
    let mut result = Vec::new();
    loop {
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
        result.push(machine.read_byte(0x87b0).unwrap());
        let eoi = machine.read_byte(0x87aa).unwrap() & 0x10 != 0;
        machine.write_byte(0x87b2, 0x98).unwrap();
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
        if eoi {
            return result;
        }
        machine.write_byte(0x87b2, 0x18).unwrap();
        assert!(result.len() < 1024, "tape response did not terminate");
    }
}

fn create_flash_storage_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        archive.start_file(*name, options).unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn create_flash_storage_archive_file(number: u8, contents: &[u8]) -> Vec<u8> {
    let header = format!(r#"[{number},"B","D",""]"#);
    create_flash_storage_archive(&[("header", header.as_bytes()), ("content", contents)])
}

// What: Poll input bytes through the last-byte EOI and subsequent empty state.
// Why: The board must see the reference's byte order and exact end-of-input signal.
#[test]
fn input_poll_marks_only_the_last_byte_and_then_returns_empty() {
    // Arrange: Start with no input, then supply three distinct bytes.
    let mut buffers = GpibBuffers::default();
    assert_eq!(buffers.poll(), (0xff, true));
    buffers.replace_input(vec![0x10, 0x20, 0xff]);

    // Act: Consume the input and poll once more after the last byte.
    let transcript: Vec<_> = (0..4).map(|_| buffers.poll()).collect();

    // Assert: EOI accompanies the final byte and every empty poll.
    assert_eq!(
        transcript,
        vec![(0x10, false), (0x20, false), (0xff, true), (0xff, true)]
    );
}

// What: Replace input after a partial read and explicitly clear an exhausted position.
// Why: The reference preserves the cursor on replacement but resets it on clear.
#[test]
fn input_replacement_preserves_position_until_cleared() {
    // Arrange: Consume two bytes of a longer transaction.
    let mut buffers = GpibBuffers::default();
    buffers.replace_input(vec![1, 2, 3, 4]);
    assert_eq!(buffers.poll(), (1, false));
    assert_eq!(buffers.poll(), (2, false));

    // Act: Replace it with a shorter input, poll, and replace it again.
    buffers.replace_input(vec![9]);
    assert_eq!(buffers.poll(), (0xff, true));
    buffers.replace_input(vec![5, 6, 7]);

    // Assert: The out-of-range poll did not reset the preserved position.
    assert_eq!(buffers.poll(), (7, true));

    // Act: Partially read another input, explicitly clear it, and replace it.
    buffers.replace_input(vec![8, 9]);
    assert_eq!(buffers.poll(), (8, false));
    buffers.clear_input();
    buffers.replace_input(vec![10, 11]);

    // Assert: Explicit clearing restarts input at the first byte.
    assert_eq!(buffers.poll(), (10, false));
    assert_eq!(buffers.poll(), (11, true));
}

// What: Accumulate, drain, and clear output independently of the WRITE byte.
// Why: Buffered output and per-byte WRITE commands use different reference paths.
#[test]
fn output_buffer_and_single_byte_are_independent() {
    // Arrange: Prepare buffered output and a separate single-byte value.
    let mut buffers = GpibBuffers::default();
    buffers.append_output(0x41);
    buffers.append_output(0x00);
    buffers.set_byte_to_send(0xfe);

    // Act: Drain the buffered output, then append and explicitly discard more.
    let output = buffers.take_output();
    buffers.append_output(0x42);
    buffers.clear_output();

    // Assert: The drain returns the original bytes, clears output, and leaves WRITE intact.
    assert_eq!(output, vec![0x41, 0x00]);
    assert!(buffers.take_output().is_empty());
    assert_eq!(buffers.byte_to_send(), 0xfe);
}

// What: Invoke the loader before and after loading a program and with another command.
// Why: Only command 0x64 should expose copied program bytes through the shared input.
#[test]
fn loader_supplies_copied_program_only_for_load_command() {
    // Arrange: An unloaded loader has no input bytes to return.
    let mut loader = Loader::default();
    let mut buffers = GpibBuffers::default();
    loader.call(0x64, &mut buffers);
    assert_eq!(buffers.poll(), (0xff, true));
    let mut program = vec![0x41, 0x00, 0x42];
    loader.load_from(&program);
    program[0] = 0x99;
    buffers.replace_input(vec![0x77]);

    // Act: Send an unrelated command, then request the loaded program.
    loader.call(0x65, &mut buffers);
    assert_eq!(buffers.poll(), (0x77, true));
    loader.call(0x64, &mut buffers);
    let transcript: Vec<_> = (0..3).map(|_| buffers.poll()).collect();

    // Assert: The program is the original copy and ends with EOI.
    assert_eq!(transcript, vec![(0x41, false), (0x00, false), (0x42, true)]);

    // Act: Replace the loaded program with an empty one and request it.
    loader.load_from(&[]);
    loader.call(0x64, &mut buffers);

    // Assert: No bytes from the previous program remain.
    assert_eq!(buffers.poll(), (0xff, true));
}

// What: Exchange a loaded program using the mapped GPIB and keyboard PIA registers.
// Why: The board must preserve the reference's DAV/NDAC and final-byte EOI transcript.
#[test]
fn machine_loader_handshake_matches_reference_pia_transcript() {
    // Arrange: Load copied bytes, select PIA data registers, and address the Loader.
    let mut machine = Machine::default();
    machine.load_program(&[0x41, 0x00, 0x42]);
    configure_gpib_talker(&mut machine);
    for byte in [0x41, 0x64] {
        machine.write_byte(0x87b0, byte).unwrap();
        machine.write_byte(0x87b1, 0x3c).unwrap();
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x00);
        machine.write_byte(0x87b1, 0x34).unwrap();
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x80);
    }
    configure_gpib_listener(&mut machine);

    // Act: Read the direction register before selecting the input data register.
    machine.write_byte(0x87b3, 0x30).unwrap();
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x0f);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0);
    machine.write_byte(0x87b3, 0x34).unwrap();

    // Act: Read the first byte twice without acknowledging DAV.
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0x41);
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);

    // Assert: DAV remains asserted, data is stable, and EOI is still low.
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0x41);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);

    // Act: Acknowledge and read the remaining two bytes, resetting the CPU in between.
    machine.write_byte(0x87b2, 0x98).unwrap();
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
    machine.reset().unwrap();
    machine.write_byte(0x87b2, 0x18).unwrap();
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0x00);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);
    machine.write_byte(0x87b2, 0x98).unwrap();
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
    machine.write_byte(0x87b2, 0x18).unwrap();

    // Assert: The final byte is available with EOI until the listener acknowledges it.
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0x42);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x1b);
    machine.write_byte(0x87b2, 0x98).unwrap();
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x0b);
}

// What: Send data to the Loader's listen address with and without keyboard EOI.
// Why: The reference dispatches buffered output only when EOI terminates the transfer.
#[test]
fn keyboard_eoi_triggers_listener_dispatch() {
    // Arrange: Prepare a Loader and its listen/secondary address in each scenario.
    for (eoi, expected) in [(false, 0xff), (true, 0x56)] {
        let mut machine = Machine::default();
        machine.load_program(&[0x56]);
        configure_gpib_talker(&mut machine);
        send_gpib_command(&mut machine, 0x21);
        send_gpib_command(&mut machine, 0x64);
        machine.write_byte(0x87b2, 0x08).unwrap();
        machine
            .write_byte(0x87aa, if eoi { 0x10 } else { 0 })
            .unwrap();

        // Act: Send one data byte, then switch the machine to listening.
        machine.write_byte(0x87b0, 0x77).unwrap();
        machine.write_byte(0x87b1, 0x3c).unwrap();
        machine.write_byte(0x87b1, 0x34).unwrap();
        configure_gpib_listener(&mut machine);

        // Assert: Only the EOI-terminated transaction supplies the program byte.
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
        assert_eq!(machine.read_byte(0x87b0).unwrap(), expected);
        assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x1b);
    }
}

// What: Attempt an unimplemented command on the default tape device.
// Why: Unsupported tape operations must not appear to work.
#[test]
fn unimplemented_tape_reports_error_on_dispatch() {
    // Arrange: Configure the default tape with an unsupported secondary command.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);

    // Act: Complete the unsupported transfer with EOI.
    let result = tape_write(&mut machine, 0x70, b"unsupported");

    // Assert: The failing device address and command are surfaced through the machine bus.
    assert_eq!(
        result,
        Err(MachineBusError::GpibCommandNotImplemented {
            address: 5,
            command: 0x70
        })
    );
}

// What: Select numbered programs, including a missing file, then issue OLD and CLOSE.
// Why: The reference snapshots the selected bytes and retains them on a missing FIND.
#[test]
fn tape_find_old_and_close_follow_reference_buffers() {
    // Arrange: Store independent programs at both valid boundaries and mutate the source.
    let mut machine = Machine::default();
    let mut first = b"10 PRINT 1\r\r".to_vec();
    machine.store_tape_program(1, &first).unwrap();
    machine.store_tape_program(254, b"20 END\r\r").unwrap();
    first[0] = b'9';
    configure_gpib_talker(&mut machine);

    // Act: Find file 1, replace its stored contents, reset the CPU, and read OLD.
    tape_find(&mut machine, b"\x0b +1suffix").unwrap();
    machine.store_tape_program(1, b"30 END\r\r").unwrap();
    machine.reset().unwrap();
    let selected = tape_read(&mut machine, 0x64);

    // Assert: OLD sends the previously selected snapshot, including after CPU reset.
    assert_eq!(selected, b"10 PRINT 1\r\r");

    // Act: Select file 1 again after its stored contents were replaced.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: The new FIND selects the replacement.
    assert_eq!(tape_read(&mut machine, 0x64), b"30 END\r\r");

    // Act: Select the maximum file number using the reference's hexadecimal syntax.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"0xfe").unwrap();
    assert_eq!(tape_read(&mut machine, 0x64), b"20 END\r\r");
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();

    // Assert: A missing in-range file leaves the previous file buffer intact.
    assert_eq!(tape_read(&mut machine, 0x64), b"20 END\r\r");

    // Act: CLOSE clears the selected file buffer.
    resume_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);

    // Assert: OLD after CLOSE returns the empty-input byte with EOI.
    assert_eq!(tape_read(&mut machine, 0x64), [0xff]);
}

// What: Request the default tape directory and reject unsupported file selections.
// Why: The root path is address-specific and invalid FIND requires an unavailable dialog.
#[test]
fn tape_directory_and_invalid_selection_are_explicit() {
    // Arrange: Prepare a default tape and reject invalid program numbers at the API boundary.
    let mut machine = Machine::default();
    for number in [0, 255] {
        assert_eq!(
            machine.store_tape_program(number, b"program"),
            Err(tek4051_emulator::machine::gpib::TapeFileNumberError(number))
        );
    }
    configure_gpib_talker(&mut machine);

    // Act: Read the default directory through the machine's PIA.
    let directory = tape_read(&mut machine, 0x69);

    // Assert: The byte response ends at the reference's address-5 root path.
    assert_eq!(directory, b"/Tape@5/");

    // Act: Find file 0, which asks the reference to open a file dialog.
    resume_gpib_talker(&mut machine);
    let result = tape_find(&mut machine, b"0");

    // Assert: The missing dialog is surfaced rather than appearing to select a file.
    assert_eq!(
        result,
        Err(MachineBusError::UnsupportedTapeSelection {
            bytes: b"0".to_vec()
        })
    );
}

// What: Read CR-terminated ASCII records, including an empty record and embedded zero.
// Why: Tape INPUT must preserve record bytes and restart its cursor only on another valid FIND.
#[test]
fn tape_input_reads_records_and_find_rewinds_the_snapshot() {
    // Arrange: Select an ASCII file with three complete records and an unterminated tail.
    let mut machine = Machine::default();
    machine
        .store_tape_program(1, b"one\0\r\rthree\rtail")
        .unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Act: Read complete records and attempt to read the remaining incomplete tail and EOF.
    for expected in [b"one\0\r".as_slice(), b"\r", b"three\r", &[0xff], &[0xff]] {
        resume_gpib_talker(&mut machine);

        // Assert: Each response includes its CR; incomplete or exhausted input polls empty with EOI.
        assert_eq!(tape_read(&mut machine, 0x6d), expected);
    }

    // Act: FIND the file again after exhausting the selected snapshot.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: The next INPUT starts at the original first record.
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\0\r");
}

// What: Request INPUT at an unterminated tail and EOF while a distinct directory response waits.
// Why: Neither condition may clear shared input or replace it with the selected file's bytes.
#[test]
fn tape_input_at_tail_and_eof_preserves_an_unread_shared_response() {
    // Arrange: Consume the complete record of a file with an unterminated tail.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"one\rtail").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");

    for _ in 0..2 {
        // Act: Queue an unread directory response before INPUT consumes the tail, then at EOF.
        resume_gpib_talker(&mut machine);
        send_gpib_command(&mut machine, 0x45);
        send_gpib_command(&mut machine, 0x69);

        // Assert: The distinct queued bytes survive both conditions through the PIA handshake.
        assert_eq!(tape_read(&mut machine, 0x6d), b"/Tape@5/");
    }
}

// What: Request INPUT with no ASCII file selected, after a missing FIND, and after CLOSE.
// Why: Retained OLD bytes must not make a missing or closed file readable as an ASCII record.
#[test]
fn tape_input_without_an_ascii_selection_returns_reference_sentinel() {
    // Arrange: Store a readable ASCII file but leave the tape initially unselected.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"one\r").unwrap();
    configure_gpib_talker(&mut machine);

    // Act: INPUT before FIND.
    let unselected = tape_read(&mut machine, 0x6d);

    // Assert: The reference sends one 0xff byte with EOI.
    assert_eq!(unselected, [0xff]);

    // Act: Select a valid file, then a missing one whose OLD snapshot is retained.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"2").unwrap();

    // Assert: INPUT reports a non-ASCII selection while OLD retains the earlier snapshot.
    assert_eq!(tape_read(&mut machine, 0x6d), [0xff]);
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x64), b"one\r");

    // Act: Select the ASCII file again, then CLOSE it.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);

    // Assert: The closed tape supplies the same sentinel rather than its old record.
    assert_eq!(tape_read(&mut machine, 0x6d), [0xff]);
}

// What: PRINT replaces the first record stream, appends later output, and creates a missing file.
// Why: Writes must update stored bytes without refreshing the selected OLD snapshot.
#[test]
fn tape_print_replaces_then_appends_and_keeps_find_snapshot() {
    // Arrange: Select an existing ASCII file at the initial record position.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Act: PRINT two record transfers and request OLD before another FIND.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"first\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"second\r").unwrap();

    // Assert: The selected OLD snapshot remains the original file.
    assert_eq!(tape_read(&mut machine, 0x64), b"old\r");

    // Act: Reselect the file and read its stored bytes.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: First PRINT replaced the old bytes, and second PRINT appended in order.
    assert_eq!(tape_read(&mut machine, 0x64), b"first\rsecond\r");

    // Act: Select a missing number, print its first contents, and FIND it again.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"254").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"created\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"254").unwrap();

    // Assert: The new ASCII file is readable as a CR-terminated record.
    assert_eq!(tape_read(&mut machine, 0x6d), b"created\r");
}

// What: Interleave INPUT and PRINT while the current file and FIND snapshot differ.
// Why: The shared cursor advances by the web's whole-buffer length and may exceed the snapshot.
#[test]
fn tape_print_and_input_share_reference_cursor_without_replacing_snapshot() {
    // Arrange: Select four short records so cursor differences produce distinct INPUT replies.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"a\rb\rc\rd\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Act: Replace stored bytes with PRINT, then INPUT from the retained snapshot at position 2.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"x\r").unwrap();

    // Assert: INPUT reads the old second record, not the newly printed bytes.
    assert_eq!(tape_read(&mut machine, 0x6d), b"b\r");

    // Act: Append a second PRINT whose whole resulting buffer advances the cursor to 8.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6c, b"y\r").unwrap();

    // Assert: INPUT is at the original snapshot's end, not at its fourth record.
    assert_eq!(tape_read(&mut machine, 0x6d), [0xff]);

    // Act: Reselect the new stream, read its first record, and append more output.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"x\r");
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6c, b"z\r").unwrap();

    // Assert: A cursor beyond the retained snapshot polls empty instead of indexing past its end.
    assert_eq!(tape_read(&mut machine, 0x6d), [0xff]);

    // Act: Refresh the snapshot once more to inspect the committed file.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: INPUT did not alter stored bytes or the append order.
    assert_eq!(tape_read(&mut machine, 0x64), b"x\ry\rz\r");

    // Act: Advance the cursor, SAVE other bytes, then PRINT without refreshing the current file.
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"x\r");
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x61, b"saved\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"q\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();

    // Assert: PRINT appends to the current FIND view, not the store bytes changed by SAVE.
    assert_eq!(tape_read(&mut machine, 0x64), b"x\ry\rz\rq\r");
}

// What: Attempt PRINT before FIND and after CLOSE.
// Why: This numbered tape subset must surface an absent file number rather than create an unnumbered entry.
#[test]
fn tape_print_requires_a_selected_file_number() {
    // Arrange: Start a fresh tape without a selected file.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);

    // Act: PRINT while no file number is selected.
    let unselected = tape_write(&mut machine, 0x6c, b"unselected\r");

    // Assert: The missing selection is an explicit machine error.
    assert_eq!(unselected, Err(MachineBusError::TapeFileNotSelected));

    // Act: Select a numbered file, close it, and PRINT again.
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);
    let closed = tape_write(&mut machine, 0x6c, b"closed\r");

    // Assert: CLOSE removes the write destination.
    assert_eq!(closed, Err(MachineBusError::TapeFileNotSelected));
}

// What: WRITE commits each binary byte immediately and leaves the prior FIND snapshot intact.
// Why: WRITE uses the separate byte path rather than waiting for a buffered transfer's EOI.
#[test]
fn tape_write_commits_before_eoi_and_preserves_binary_bytes() {
    // Arrange: Retain an ASCII snapshot while selecting a missing file at cursor zero.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"2").unwrap();

    // Act: Send one WRITE byte with no EOI, then request binary READ immediately.
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x25);
    send_gpib_command(&mut machine, 0x6f);
    machine.write_byte(0x87b2, 0x08).unwrap();
    machine.write_byte(0x87aa, 0).unwrap();
    machine.write_byte(0x87b0, 0x80).unwrap();
    machine.write_byte(0x87b1, 0x3c).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();

    // Assert: The byte was committed and the new file is already binary-readable.
    assert_eq!(tape_read(&mut machine, 0x6e), [0x80]);

    // Act: Append zero, 0xff, and CR through another WRITE transfer.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6f, &[0x00, 0xff, 0x0d]).unwrap();

    // Assert: OLD still returns the preceding ASCII snapshot, not the binary working contents.
    assert_eq!(tape_read(&mut machine, 0x64), b"old\r");
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6e), [0x80, 0x00, 0xff, 0x0d]);
}

// What: Rewrite binary data across transfers and read current contents once per FIND.
// Why: Only the first WRITE clears data, READ uses the working file, and repeated READ retains input.
#[test]
fn tape_binary_rewrite_and_read_follow_current_file_and_one_shot_cursor() {
    // Arrange: Create binary data and select its snapshot.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[1, 2, 3]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();
    let rewritten: Vec<u8> = (9..=110).collect();

    // Act: Rewrite with one byte, reset the CPU, then append beyond READ's consumed marker.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[9]).unwrap();
    machine.reset().unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &rewritten[1..]).unwrap();

    // Assert: READ exposes rewritten contents while OLD retains the selected snapshot.
    assert_eq!(tape_read(&mut machine, 0x6e), rewritten);
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x64), [1, 2, 3]);

    // Act: Queue a distinct directory reply before repeating READ without FIND.
    resume_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x69);

    // Assert: Repeated READ supplies no replacement and does not clear the pending response.
    assert_eq!(tape_read(&mut machine, 0x6e), b"/Tape@5/");

    // Act: FIND again to rewind READ and arm the next WRITE clear.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: A new FIND makes the full rewritten binary file readable again.
    assert_eq!(tape_read(&mut machine, 0x6e), rewritten);
}

// What: Apply ASCII and binary commands to incompatible types and SAVE over binary data.
// Why: Type guards and SAVE's separate store update must not reinterpret the current file.
#[test]
fn tape_binary_and_ascii_type_guards_preserve_save_current_file_separation() {
    // Arrange: Select an ASCII program, whose WRITE clear has not been armed.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"one\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Act: Send binary WRITE and request binary READ against ASCII.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0x80]).unwrap();

    // Assert: READ returns the type sentinel without advancing the ASCII INPUT cursor.
    assert_eq!(tape_read(&mut machine, 0x6e), [0xff]);
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");

    // Act: Create binary data at a missing number, then try ASCII PRINT and INPUT.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0x80, 0x00]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"ignored\r").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), [0xff]);

    // Act: SAVE ASCII bytes without refreshing the current binary selection.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x61, b"saved\r").unwrap();

    // Assert: READ still returns the binary working file, not PRINT or SAVE output.
    assert_eq!(tape_read(&mut machine, 0x6e), [0x80, 0x00]);

    // Act: Reselect the SAVE result and request both read modes.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();

    // Assert: FIND now selects ASCII; READ sends the sentinel and INPUT reads saved bytes.
    assert_eq!(tape_read(&mut machine, 0x6e), [0xff]);
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"saved\r");
}

// What: Arm a WRITE clear with ASCII data, CLOSE, then WRITE to a selected ASCII program.
// Why: The reference retains its private clear flag and clears the working file before the type guard.
#[test]
fn tape_rejected_ascii_write_consumes_the_retained_data_clear() {
    // Arrange: Create ASCII data, select it to arm clearing, then CLOSE.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"one\rtwo\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"data\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");

    // Act: WRITE is type-rejected after consuming the retained clear flag.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6f, &[0x80]).unwrap();

    // Assert: The stored snapshot remains unchanged by the rejected WRITE.
    assert_eq!(tape_read(&mut machine, 0x64), b"one\rtwo\r");

    // Act: PRINT at the advanced cursor, then reselect the committed file.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6c, b"q\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();

    // Assert: PRINT appended to the cleared working file, not the retained snapshot.
    assert_eq!(tape_read(&mut machine, 0x64), b"q\r");
}

// What: Create binary data while a missing FIND retains a nonzero ASCII record cursor.
// Why: WRITE must not rewind the shared cursor, so READ needs a fresh FIND before sending bytes.
#[test]
fn tape_write_keeps_the_cursor_retained_by_a_missing_find() {
    // Arrange: Advance an ASCII cursor and select a missing numbered file.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"one\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();

    // Act: WRITE binary data without a cursor rewind.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0x80]).unwrap();

    // Assert: READ supplies no response at the retained nonzero cursor.
    assert_eq!(tape_read(&mut machine, 0x6e), [0xff]);
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();

    // Assert: FIND rewinds and makes the previously written byte readable.
    assert_eq!(tape_read(&mut machine, 0x6e), [0x80]);
}

// What: Request READ and WRITE before FIND and after CLOSE.
// Why: READ supplies the reference type sentinel while numbered WRITE requires a selected file.
#[test]
fn tape_binary_commands_report_missing_and_closed_selections() {
    // Arrange: Start an unselected tape and inspect its READ response.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6e), [0xff]);
    resume_gpib_talker(&mut machine);

    // Act: WRITE without a selected file.
    let unselected = tape_write(&mut machine, 0x6f, &[0x80]);

    // Assert: The missing number is surfaced as an explicit machine error.
    assert_eq!(unselected, Err(MachineBusError::TapeFileNotSelected));

    // Act: Recover the handshake, select a file, CLOSE, and READ.
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);
    let closed_read = tape_read(&mut machine, 0x6e);
    resume_gpib_talker(&mut machine);
    let closed_write = tape_write(&mut machine, 0x6f, &[0x80]);

    // Assert: CLOSE restores the sentinel for READ and the selection error for WRITE.
    assert_eq!(closed_read, [0xff]);
    assert_eq!(closed_write, Err(MachineBusError::TapeFileNotSelected));
}

// What: BSAVE creates a binary program without changing the existing selection until FIND.
// Why: BOLD requires binary-program metadata and sends a padded snapshot rather than current store bytes.
#[test]
fn tape_bsave_replaces_store_and_bold_waits_for_find() {
    // Arrange: Select an ASCII program before saving distinct binary bytes over it.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    let program = [0x00, 0xff, 0x80];
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x71, &program).unwrap();

    // Act: Queue a directory reply and request BOLD before refreshing the ASCII selection.
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x69);

    // Assert: BOLD ignores the current ASCII type and OLD retains its original snapshot.
    assert_eq!(tape_read(&mut machine, 0x71), b"/Tape@5/");
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x64), b"old\r");

    // Act: FIND selects the newly saved binary program and BOLD requests its padded transfer.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    let mut expected = vec![b' '; 256];
    expected[..program.len()].copy_from_slice(&program);
    expected[program.len()] = 0xff;

    // Assert: BOLD preserves all binary bytes and pads through the final EOI handshake.
    assert_eq!(tape_read(&mut machine, 0x71), expected);
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6e), program);

    // Act: Request BOLD again after READ has consumed its shared cursor.
    resume_gpib_talker(&mut machine);

    // Assert: BOLD is repeatable and does not use READ's one-shot cursor guard.
    assert_eq!(tape_read(&mut machine, 0x71), expected);
}

// What: Interrupt BSAVE before EOI and reselect the original file.
// Why: Binary program saves must commit complete buffered transfers, not individual bytes.
#[test]
fn tape_bsave_does_not_commit_before_eoi() {
    // Arrange: Select an ASCII file and start the binary save listener command.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x25);
    send_gpib_command(&mut machine, 0x71);
    machine.write_byte(0x87b2, 0x08).unwrap();
    machine.write_byte(0x87aa, 0).unwrap();

    // Act: Send a binary byte without EOI, then abandon the transfer with FIND.
    machine.write_byte(0x87b0, 0x80).unwrap();
    machine.write_byte(0x87b1, 0x3c).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();

    // Assert: Neither stored bytes nor the ASCII type changed before EOI.
    assert_eq!(tape_read(&mut machine, 0x64), b"old\r");
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"old\r");
}

// What: Interleave BSAVE, PRINT, WRITE, READ, and BOLD on a selected binary program.
// Why: Program usage, current contents, the snapshot, and READ's consumed cursor must stay distinct.
#[test]
fn tape_binary_program_commands_preserve_kind_snapshot_and_cursor() {
    // Arrange: Create and FIND a binary program without a pending data-file WRITE clear.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x71, &[0x11, 0x22]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();

    // Act: BSAVE other store bytes, try ignored ASCII PRINT, then append binary WRITE.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x71, &[0x33]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"ignored\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0x44]).unwrap();

    // Assert: Program FIND did not arm clearing, and WRITE appended to current rather than BSAVE bytes.
    assert_eq!(tape_read(&mut machine, 0x6e), [0x11, 0x22, 0x44]);
    resume_gpib_talker(&mut machine);
    let mut expected = vec![b' '; 256];
    expected[..3].copy_from_slice(&[0x11, 0x22, 0xff]);
    assert_eq!(tape_read(&mut machine, 0x71), expected);

    // Act: Queue a distinct response before READ after BOLD.
    resume_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x69);

    // Assert: BOLD did not reset the cursor already consumed by READ.
    assert_eq!(tape_read(&mut machine, 0x6e), b"/Tape@5/");

    // Act: Reselect the WRITE result and request BOLD again.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    expected[..4].copy_from_slice(&[0x11, 0x22, 0x44, 0xff]);

    // Assert: WRITE retained binary-program usage, and only FIND refreshed BOLD's snapshot.
    assert_eq!(tape_read(&mut machine, 0x71), expected);
}

// What: Request BOLD with unselected, data, ASCII, missing, and closed file states.
// Why: Only a selected binary program may replace shared input, even when OLD bytes were retained.
#[test]
fn tape_bold_ignores_non_program_and_missing_selections() {
    // Arrange: Create binary data, ASCII data/programs, and a binary program in one tape.
    let mut machine = Machine::default();
    machine.store_tape_program(4, b"ascii\r").unwrap();
    configure_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x69);
    assert_eq!(tape_read(&mut machine, 0x71), b"/Tape@5/");
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x71, &[0x80]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0x81]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"3").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"data\r").unwrap();

    for number in [b"2", b"3", b"4", b"5"] {
        // Act: Retain a binary-program snapshot, then select an incompatible or missing file.
        machine.write_byte(0x87b2, 0).unwrap();
        tape_find(&mut machine, b"1").unwrap();
        machine.write_byte(0x87b2, 0).unwrap();
        tape_find(&mut machine, number).unwrap();
        machine.write_byte(0x87b2, 0).unwrap();
        send_gpib_command(&mut machine, 0x45);
        send_gpib_command(&mut machine, 0x69);

        // Assert: BOLD preserves the distinct queued reply rather than sending data or 0xff.
        assert_eq!(tape_read(&mut machine, 0x71), b"/Tape@5/");
        resume_gpib_talker(&mut machine);
    }

    // Act: Select the binary program, CLOSE it, and queue another directory response.
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);
    send_gpib_command(&mut machine, 0x69);

    // Assert: CLOSE removes BOLD eligibility and still leaves unrelated shared input alone.
    assert_eq!(tape_read(&mut machine, 0x71), b"/Tape@5/");
}

// What: Attempt BSAVE before FIND and after CLOSE.
// Why: The numbered Rust subset must reject an absent file number explicitly.
#[test]
fn tape_bsave_requires_a_selected_file_number() {
    // Arrange: Start a fresh tape without selecting a numbered file.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);

    // Act: BSAVE an unselected file.
    let unselected = tape_write(&mut machine, 0x71, &[0x80]);

    // Assert: The missing selection is reported at the machine boundary.
    assert_eq!(unselected, Err(MachineBusError::TapeFileNotSelected));

    // Act: Recover the handshake, select a number, CLOSE, and BSAVE again.
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);
    let closed = tape_write(&mut machine, 0x71, &[0x80]);

    // Assert: CLOSE removes the selected destination.
    assert_eq!(closed, Err(MachineBusError::TapeFileNotSelected));
}

// What: Read LIST metadata for the four program/data kinds created without headers.
// Why: File number, type, usage, blank description, and byte size have exact reference field widths.
#[test]
fn tape_list_reads_fixed_width_metadata_for_supported_kinds() {
    // Arrange: Store an ASCII program and create ASCII/binary data and a binary program.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"10 END\r\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"data\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"3").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6f, &[0x00, 0xff, 0x80]).unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"254").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x71, &[0x81; 256]).unwrap();

    for (number, expected) in [
        (
            b"1".as_slice(),
            concat!("1      ", "ASCII   ", "PROG ", "                ", " ", "8"),
        ),
        (
            b"2".as_slice(),
            concat!("2      ", "ASCII   ", "DATA ", "                ", " ", "5"),
        ),
        (
            b"3".as_slice(),
            concat!("3      ", "BINARY  ", "DATA ", "                ", " ", "3"),
        ),
        (
            b"254".as_slice(),
            concat!(
                "254    ",
                "BINARY  ",
                "PROG ",
                "                ",
                " ",
                "256"
            ),
        ),
    ] {
        // Act: Select the file and request LIST through the PIA handshake.
        machine.write_byte(0x87b2, 0).unwrap();
        tape_find(&mut machine, number).unwrap();
        let actual = tape_read(&mut machine, 0x73);

        // Assert: All field bytes and the unpadded size match, without a trailing CR or NUL.
        assert_eq!(actual, expected.as_bytes());
        resume_gpib_talker(&mut machine);
    }
}

// What: Read LIST after SAVE and an ignored ASCII WRITE change store and working state separately.
// Why: Metadata must describe the selected current file, not re-fetch stored type or byte size.
#[test]
fn tape_list_uses_current_kind_and_size_until_find() {
    // Arrange: Create and select ASCII data so the next WRITE has a pending clear.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"data\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();

    // Act: SAVE an ASCII program over the store without refreshing the current data file.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x61, b"10 END\r\r").unwrap();

    // Assert: LIST still reports the data view and its original size.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("1      ", "ASCII   ", "DATA ", "                ", " ", "5").as_bytes()
    );

    // Act: Rejected ASCII WRITE consumes the pending clear of working contents.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6f, &[0x80]).unwrap();

    // Assert: LIST observes the cleared working size but still reports the data kind.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("1      ", "ASCII   ", "DATA ", "                ", " ", "0").as_bytes()
    );

    // Act: FIND refreshes from the saved program in the store.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: LIST now reports program usage and the saved size.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("1      ", "ASCII   ", "PROG ", "                ", " ", "8").as_bytes()
    );
}

// What: Read LIST before selection, after missing FIND/SAVE, and after CLOSE.
// Why: Retained bytes or a new store entry must not make an untyped current selection look marked.
#[test]
fn tape_list_returns_sentinel_for_untyped_missing_and_closed_selections() {
    // Arrange: Store a file but leave the tape initially unselected.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);

    // Act: LIST without a selection.
    let unselected = tape_read(&mut machine, 0x73);

    // Assert: The missing current file sends one 0xff with EOI.
    assert_eq!(unselected, [0xff]);

    // Act: Select a valid file and then a missing number that retains its OLD snapshot.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"2").unwrap();

    // Assert: LIST does not report the retained OLD file as the current marked file.
    assert_eq!(tape_read(&mut machine, 0x73), [0xff]);

    // Act: SAVE creates a store record but leaves the current file untyped until FIND.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x61, b"new\r").unwrap();

    // Assert: Mere store existence does not make LIST return a marked header.
    assert_eq!(tape_read(&mut machine, 0x73), [0xff]);

    // Act: FIND the stored record, then CLOSE and request LIST again.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);

    // Assert: CLOSE removes metadata eligibility.
    assert_eq!(tape_read(&mut machine, 0x73), [0xff]);
}

// What: Commit a renumbered header only at EOI, then SAVE using the original FIND number.
// Why: LIST changes current metadata but not the web fileNum control used by SAVE/BSAVE.
#[test]
fn tape_list_header_commits_at_eoi_without_changing_save_target() {
    // Arrange: Select file 1 and send all but the last byte of a header for file 7.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"one\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    send_gpib_command(&mut machine, 0x25);
    send_gpib_command(&mut machine, 0x73);
    machine.write_byte(0x87b2, 0x08).unwrap();
    machine.write_byte(0x87aa, 0).unwrap();
    let header = b"7      ASCII   TEXT [Label] 0";
    for &byte in &header[..header.len() - 1] {
        send_gpib_command(&mut machine, byte);
    }

    // Act: Read metadata while the outbound header has not received EOI.
    let before_eoi = tape_read(&mut machine, 0x73);

    // Assert: No header change has been committed.
    assert_eq!(
        before_eoi,
        concat!("1      ASCII   PROG ", "                ", " 4").as_bytes()
    );

    // Act: Restore just the listen address, retaining buffered output, and send the final byte.
    resume_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x25);
    machine.write_byte(0x87b2, 0x08).unwrap();
    machine.write_byte(0x87aa, 0x10).unwrap();
    send_gpib_command(&mut machine, b'0');

    // Assert: The new key describes unchanged contents, not the supplied size field.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("7      ASCII   TEXT ", "Label           ", " 4").as_bytes()
    );

    // Act: SAVE after renumbering, then select each store entry.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x61, b"newer\r").unwrap();
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("7      ASCII   TEXT ", "Label           ", " 4").as_bytes()
    );
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Assert: SAVE replaced file 1 and reset its stored description; file 7 remains a copy.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("1      ASCII   PROG ", "                ", " 6").as_bytes()
    );
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"7").unwrap();
    assert_eq!(tape_read(&mut machine, 0x64), b"one\r");
}

// What: Apply a binary-program header to changed working data without rewinding or disarming WRITE.
// Why: LIST saves current contents while OLD/BOLD retain the FIND snapshot and pending clear state.
#[test]
fn tape_list_header_preserves_snapshot_cursor_and_pending_write_clear() {
    // Arrange: Create data, rewind it, consume a record, append, and independently replace the store.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x6c, b"one\rtwo\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6c, b"end\r").unwrap();
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(&mut machine, 0x61, b"saved\r").unwrap();

    // Act: Renumber and reclassify the working bytes, removing brackets before truncation.
    machine.write_byte(0x87b2, 0).unwrap();
    tape_write(
        &mut machine,
        0x73,
        b"2      BINARY  PROG [LongDescriptionHere] 999",
    )
    .unwrap();

    // Assert: LIST saved the 12 working bytes, with the exact truncated description.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        b"2      BINARY  PROG LongDescriptionH 12"
    );

    // Act: Queue a distinct reply before READ, then request BOLD.
    resume_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x69);

    // Assert: READ did not rewind; BOLD still loads the eight-byte FIND snapshot.
    assert_eq!(tape_read(&mut machine, 0x6e), b"/Tape@5/");
    resume_gpib_talker(&mut machine);
    let binary_program = tape_read(&mut machine, 0x71);
    assert_eq!(&binary_program[..9], b"one\rtwo\r\xff");
    assert_eq!(binary_program.len(), 256);
    assert!(binary_program[9..].iter().all(|&byte| byte == b' '));

    // Act: WRITE after the header consumes the data FIND's pending clear.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x6f, &[0xaa]).unwrap();

    // Assert: Only the new byte remains current/stored; the OLD snapshot is still unchanged.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        b"2      BINARY  PROG LongDescriptionH 1"
    );
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x64), b"one\rtwo\r");
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();
    assert_eq!(tape_read(&mut machine, 0x64), [0xaa]);
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x64), b"saved\r");
}

// What: Parse numeric prefixes, only the first type/usage letters, and raw description bytes.
// Why: The web parser does not validate full labels, UTF-8, trailing size, or a missing separator.
#[test]
fn tape_list_header_parser_matches_reference_byte_and_position_rules() {
    // Arrange: Use distinct parser edge cases and independently specified expected wire entries.
    for (header, expected) in [
        (
            b"0xfe   ASCII   LOG  [A\xffB] 999".as_slice(),
            [
                b"254    ASCII   LOG  ".as_slice(),
                b"A\xffB",
                b"             ",
                b" 4",
            ]
            .concat(),
        ),
        (
            b"0000019B???????TxxxxabcdefghijklmnopQRST bogus",
            b"1      BINARY  TEXT abcdefghijklmnop 4".to_vec(),
        ),
        (
            b"2junk  A???????Pxxxxabcdefghijklmnop",
            concat!("2      ASCII   PROG ", "                ", " 4")
                .as_bytes()
                .to_vec(),
        ),
    ] {
        let mut machine = Machine::default();
        machine.store_tape_program(1, b"old\r").unwrap();
        configure_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();

        // Act: Send the header and read the resulting marked entry through the PIA.
        machine.write_byte(0x87b2, 0).unwrap();
        tape_write(&mut machine, 0x73, header).unwrap();
        let actual = tape_read(&mut machine, 0x73);

        // Assert: Position/prefix rules and Latin-1 bytes match without using the stated size.
        assert_eq!(actual, expected, "header {header:?}");
    }
}

// What: Ignore out-of-range numeric header numbers or establish a current number without FIND.
// Why: tFile.num keeps its prior number; an absent SAVE control is still an explicit Rust error.
#[test]
fn tape_list_header_number_setter_and_unselected_save_follow_reference_state() {
    for number in ["0", "255", "-1", "999999"] {
        // Arrange: Select a numbered program before supplying a numeric but out-of-range header.
        let mut machine = Machine::default();
        machine.store_tape_program(1, b"old\r").unwrap();
        configure_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();

        // Act: Change type/usage/description while the number setter rejects the new number.
        machine.write_byte(0x87b2, 0).unwrap();
        let header = format!("{number:<7}BINARY  DATA [Keep] 123");
        tape_write(&mut machine, 0x73, header.as_bytes()).unwrap();

        // Assert: The old number remains valid and the other header fields changed.
        assert_eq!(
            tape_read(&mut machine, 0x73),
            concat!("1      BINARY  DATA ", "Keep            ", " 4").as_bytes()
        );
    }

    // Arrange: Leave the tape unselected despite an unrelated stored file.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r").unwrap();
    configure_gpib_talker(&mut machine);

    // Act: A valid header with no usable number cannot create an unnumbered Rust entry.
    let result = tape_write(&mut machine, 0x73, b"0      ASCII   PROG [Empty] 0");

    // Assert: The failure is explicit and leaves the current selection untyped.
    assert_eq!(result, Err(MachineBusError::TapeFileNotSelected));
    machine.write_byte(0x87b1, 0x34).unwrap();
    assert_eq!(tape_read(&mut machine, 0x73), [0xff]);

    // Act: A numbered header creates an empty current file without setting SAVE's FIND number.
    resume_gpib_talker(&mut machine);
    tape_write(&mut machine, 0x73, b"7      ASCII   PROG [Empty] 99").unwrap();
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("7      ASCII   PROG ", "Empty           ", " 0").as_bytes()
    );
    resume_gpib_talker(&mut machine);
    let save = tape_write(&mut machine, 0x61, b"not saved\r");

    // Assert: SAVE remains unselected, and the newly marked current file still exists.
    assert_eq!(save, Err(MachineBusError::TapeFileNotSelected));
    machine.write_byte(0x87b1, 0x34).unwrap();
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("7      ASCII   PROG ", "Empty           ", " 0").as_bytes()
    );
}

// What: Reject malformed headers without changing current metadata, records, or cursor.
// Why: Rust surfaces invalid input rather than silently ignoring it like the web.
#[test]
fn invalid_tape_list_headers_are_explicit_and_leave_state_unchanged() {
    for header in [
        b"abc    ASCII   PROG [Bad] 0".as_slice(),
        b"1      QSCII   PROG [Bad] 0",
        b"1      ASCII   ?ROG [Bad] 0",
        b"header",
    ] {
        // Arrange: Select a two-record file and consume its first record.
        let mut machine = Machine::default();
        machine.store_tape_program(1, b"one\rtwo\r").unwrap();
        configure_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();
        assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");

        // Act: Finish a malformed header transaction with EOI.
        resume_gpib_talker(&mut machine);
        let result = tape_write(&mut machine, 0x73, header);

        // Assert: The exact invalid bytes are reported; metadata and the record cursor survive.
        assert_eq!(
            result,
            Err(MachineBusError::InvalidTapeHeader {
                bytes: header.to_vec()
            })
        );
        machine.write_byte(0x87b1, 0x34).unwrap();
        assert_eq!(
            tape_read(&mut machine, 0x73),
            concat!("1      ASCII   PROG ", "                ", " 8").as_bytes()
        );
        resume_gpib_talker(&mut machine);
        assert_eq!(tape_read(&mut machine, 0x6d), b"two\r");
        resume_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();
        assert_eq!(tape_read(&mut machine, 0x64), b"one\rtwo\r");
    }
}

// What: Turn NEW into ASCII/binary data and reject writes to LAST without rewinding marker FINDs.
// Why: Marker headers preserve usage, contents, snapshot, and cursor until a permitted data write.
#[test]
fn tape_new_and_last_markers_preserve_reference_transfer_rules() {
    for (command, output, label, size) in [
        (0x6c, b"x\r".as_slice(), "ASCII", 10),
        (0x6f, &[0x80], "BINARY", 9),
    ] {
        // Arrange: Consume a record before reclassifying the file as NEW with an ignored usage.
        let mut machine = Machine::default();
        machine.store_tape_program(1, b"one\rtwo\r").unwrap();
        configure_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();
        assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");
        resume_gpib_talker(&mut machine);
        tape_write(&mut machine, 0x73, b"1      NEW     ???? [Marker] 0").unwrap();
        assert_eq!(
            tape_read(&mut machine, 0x73),
            concat!("1      NEW     PROG ", "Marker          ", " 8").as_bytes()
        );

        // Act: FIND the marker and attempt INPUT before a permitted PRINT or WRITE.
        resume_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();
        assert_eq!(tape_read(&mut machine, 0x6d), [0xff]);
        resume_gpib_talker(&mut machine);
        tape_write(&mut machine, command, output).unwrap();

        // Assert: The existing bytes were appended, with data usage and description retained.
        assert_eq!(
            tape_read(&mut machine, 0x73),
            format!("1      {label:<8}DATA Marker           {size}").as_bytes()
        );

        // Act: Mark LAST, FIND it, and attempt both data writes.
        resume_gpib_talker(&mut machine);
        tape_write(&mut machine, 0x73, b"1      LAST    ???? [Marker] 0").unwrap();
        machine.write_byte(0x87b2, 0).unwrap();
        tape_find(&mut machine, b"1").unwrap();
        machine.write_byte(0x87b2, 0).unwrap();
        tape_write(&mut machine, 0x6c, b"ignored\r").unwrap();
        machine.write_byte(0x87b2, 0).unwrap();
        tape_write(&mut machine, 0x6f, &[0xff]).unwrap();

        // Assert: LAST did not arm WRITE clearing or refresh OLD's earlier eight-byte snapshot.
        assert_eq!(
            tape_read(&mut machine, 0x73),
            format!("1      LAST    DATA Marker           {size}").as_bytes()
        );
        resume_gpib_talker(&mut machine);
        assert_eq!(tape_read(&mut machine, 0x64), b"one\rtwo\r");
        resume_gpib_talker(&mut machine);
        assert_eq!(tape_read(&mut machine, 0x6e), [0xff]);
    }

    // Arrange: Start without a selected file or prior usage.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    for (header, label) in [(b"8      NEW".as_slice(), "NEW"), (b"8      LAST", "LAST")] {
        // Act: Write a short marker header without any usage or description field.
        tape_write(&mut machine, 0x73, header).unwrap();

        // Assert: A numbered marker is a valid LIST entry even with no usage or contents.
        assert_eq!(
            tape_read(&mut machine, 0x73),
            format!("8      {label:<8}                      0").as_bytes()
        );
        resume_gpib_talker(&mut machine);
    }
}

// What: Transfer ASCII/binary files marked LOG or TEXT without changing their usage.
// Why: Type gates data transfers, only DATA FIND arms clearing, and BOLD requires PROG usage.
#[test]
fn tape_log_and_text_usage_survives_data_transfers() {
    for usage in ["LOG", "TEXT"] {
        for file_type in ["ASCII", "BINARY"] {
            // Arrange: Reclassify a stored file, then FIND its header and contents.
            let mut machine = Machine::default();
            machine.store_tape_program(1, b"old\r").unwrap();
            configure_gpib_talker(&mut machine);
            tape_find(&mut machine, b"1").unwrap();
            machine.write_byte(0x87b2, 0).unwrap();
            let header = format!("1      {file_type:<8}{usage:<5}[Notes] 0");
            tape_write(&mut machine, 0x73, header.as_bytes()).unwrap();
            machine.write_byte(0x87b2, 0).unwrap();
            tape_find(&mut machine, b"1").unwrap();

            // Act: PRINT replaces ASCII at cursor zero; WRITE appends binary without DATA clearing.
            machine.write_byte(0x87b2, 0).unwrap();
            if file_type == "ASCII" {
                tape_write(&mut machine, 0x6c, b"new\r").unwrap();
            } else {
                tape_write(&mut machine, 0x6f, &[0xaa]).unwrap();
            }

            // Assert: The usage/description and expected working size survive both paths.
            let size = if file_type == "ASCII" { 4 } else { 5 };
            assert_eq!(
                tape_read(&mut machine, 0x73),
                format!("1      {file_type:<8}{usage:<5}Notes            {size}").as_bytes()
            );

            // Act: Rewind and read through the appropriate type-specific operation.
            resume_gpib_talker(&mut machine);
            tape_find(&mut machine, b"1").unwrap();

            // Assert: Both non-program usages support data reads, but not binary program loading.
            if file_type == "ASCII" {
                assert_eq!(tape_read(&mut machine, 0x6d), b"new\r");
            } else {
                assert_eq!(tape_read(&mut machine, 0x6e), b"old\r\xaa");
                resume_gpib_talker(&mut machine);
                send_gpib_command(&mut machine, 0x45);
                send_gpib_command(&mut machine, 0x69);
                assert_eq!(tape_read(&mut machine, 0x71), b"/Tape@5/");
            }
        }
    }
}

// What: Import mixed Flash Drive files, retaining old storage and replacing duplicate numbers in order.
// Why: Valid files must survive neighboring failures and select the first entry in the whole store.
#[test]
fn flash_import_keeps_successes_reports_failures_and_selects_first_stored_file() {
    // Arrange: Consume a record from file 4 while retaining a lower file absent from the import batch.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"retained\r").unwrap();
    machine.store_tape_program(4, b"one\rtwo\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"4").unwrap();
    assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");
    let file = |name: &str, bytes: &[u8]| FlashDriveFile {
        name: name.to_owned(),
        bytes: bytes.to_vec(),
    };
    let files = [
        file("2      BINARY  DATA [First] 999.bin", &[0, 0xff]),
        file("0      ASCII   PROG [Invalid] 0", b"rejected"),
        file("2      ASCII   TEXT [Updated] 999.txt", b"new\r"),
        file("254    NEW          [Free] 0", &[]),
        file("not a Flash Drive filename", b"rejected"),
        file("3      LAST         [End] 0", &[]),
    ];

    // Act: Import in input order and observe each result through the public machine boundary.
    let results = machine.import_flash_files(&files);

    // Assert: Valid results survive failures, and selection changes to the lowest whole-store entry.
    assert_eq!(
        results,
        [
            Ok(()),
            Err(FlashDriveImportError::FileNumberOutOfRange),
            Ok(()),
            Ok(()),
            Err(FlashDriveImportError::InvalidFilename),
            Ok(())
        ]
    );
    resume_gpib_talker(&mut machine);
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("1      ASCII   PROG ", "                ", " 9").as_bytes()
    );
    // Assert: INPUT still consumes file 4's snapshot/cursor despite selecting different current contents.
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x6d), b"two\r");

    // Act: Select the duplicate number and then the independently imported marker files.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"2").unwrap();

    // Assert: The last duplicate wins with unchanged payload and metadata; markers have fresh blank usage.
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("2      ASCII   TEXT ", "Updated         ", " 4").as_bytes()
    );
    resume_gpib_talker(&mut machine);
    assert_eq!(tape_read(&mut machine, 0x64), b"new\r");
    for (number, expected) in [
        (
            b"254".as_slice(),
            concat!("254    NEW     ", "     ", "Free            ", " 0"),
        ),
        (
            b"3".as_slice(),
            concat!("3      LAST    ", "     ", "End             ", " 0"),
        ),
    ] {
        resume_gpib_talker(&mut machine);
        tape_find(&mut machine, number).unwrap();
        assert_eq!(tape_read(&mut machine, 0x73), expected.as_bytes());
    }
}

// What: Select imported binary data while a different ASCII snapshot/cursor and WRITE clear remain.
// Why: Browser import selection neither refreshes the snapshot nor arms or cancels DATA clearing.
#[test]
fn flash_import_preserves_transfer_snapshot_cursor_and_write_clear() {
    for pending_clear in [false, true] {
        // Arrange: Consume a record of either a program or data file at number 2.
        let mut machine = Machine::default();
        configure_gpib_talker(&mut machine);
        if pending_clear {
            tape_find(&mut machine, b"2").unwrap();
            machine.write_byte(0x87b2, 0).unwrap();
            tape_write(&mut machine, 0x6c, b"one\rtwo\r").unwrap();
            machine.write_byte(0x87b2, 0).unwrap();
        } else {
            machine.store_tape_program(2, b"one\rtwo\r").unwrap();
        }
        tape_find(&mut machine, b"2").unwrap();
        assert_eq!(tape_read(&mut machine, 0x6d), b"one\r");

        // Act: Import lower-numbered data; the current view changes but READ's cursor does not.
        assert_eq!(
            machine.import_flash_files(&[FlashDriveFile {
                name: "1      BINARY  DATA [Imported] 0".to_owned(),
                bytes: vec![0x55, 0x66],
            }]),
            [Ok(())]
        );
        resume_gpib_talker(&mut machine);
        send_gpib_command(&mut machine, 0x45);
        send_gpib_command(&mut machine, 0x69);

        // Assert: READ retains the queued reply because import did not rewind the consumed cursor.
        assert_eq!(tape_read(&mut machine, 0x6e), b"/Tape@5/");

        // Act: WRITE to the imported file, then observe the retained OLD snapshot and stored data.
        resume_gpib_talker(&mut machine);
        tape_write(&mut machine, 0x6f, &[0xaa]).unwrap();

        // Assert: Only the preceding data FIND clears; imported DATA did not newly arm that flag.
        assert_eq!(tape_read(&mut machine, 0x64), b"one\rtwo\r");
        resume_gpib_talker(&mut machine);
        tape_find(&mut machine, b"1").unwrap();
        let expected = if pending_clear {
            vec![0xaa]
        } else {
            vec![0x55, 0x66, 0xaa]
        };
        assert_eq!(tape_read(&mut machine, 0x6e), expected);
    }
}

// What: Parse Unicode filenames using UTF-16 positions before converting LIST descriptions to bytes.
// Why: Premature UTF-8/low-byte conversion changes brackets, type letters, and surrogate truncation.
#[test]
fn flash_import_unicode_names_match_javascript_character_rules() {
    // Arrange: Include non-ASCII description units, Unicode whitespace, a split surrogate, and a type alias.
    let mut machine = Machine::default();
    let files = [
        FlashDriveFile {
            name: "2      ASCII   TEXT [A\u{15b}\u{1f600}B] 999.txt".to_owned(),
            bytes: vec![1, 2],
        },
        FlashDriveFile {
            name: "\u{2003}3     ASCII   LOG  [Space] 0".to_owned(),
            bytes: vec![3],
        },
        FlashDriveFile {
            name: "4      ASCII   TEXT [abcdefghijklmno\u{1f600}] 0".to_owned(),
            bytes: vec![4],
        },
        FlashDriveFile {
            name: "5      \u{141}AST         [Alias] 0".to_owned(),
            bytes: vec![5],
        },
    ];

    // Act: Import the names without converting their Unicode units to bus bytes during parsing.
    let results = machine.import_flash_files(&files);

    // Assert: Real Unicode metadata parses, but a Unicode letter whose low byte is L is not LAST.
    assert_eq!(
        results,
        [
            Ok(()),
            Ok(()),
            Ok(()),
            Err(FlashDriveImportError::InvalidFilename)
        ]
    );
    configure_gpib_talker(&mut machine);
    assert_eq!(
        tape_read(&mut machine, 0x73),
        [
            b"2      ASCII   TEXT ".as_slice(),
            b"A[=\0B",
            b"           ",
            b" 2"
        ]
        .concat()
    );
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"3").unwrap();
    assert_eq!(
        tape_read(&mut machine, 0x73),
        concat!("3      ASCII   LOG  ", "Space           ", " 1").as_bytes()
    );
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"4").unwrap();
    assert_eq!(
        tape_read(&mut machine, 0x73),
        b"4      ASCII   TEXT abcdefghijklmno= 1"
    );
}

// What: SAVE replaces a numbered program but does not refresh the FIND snapshot.
// Why: The reference exposes saved bytes only after the file is selected again.
#[test]
fn tape_save_replaces_program_on_eoi_and_find_refreshes_snapshot() {
    // Arrange: Select an existing program through the tape PIA.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();

    // Act: Send a replacement program ending with EOI and read without another FIND.
    machine.write_byte(0x87b2, 0x00).unwrap();
    tape_write(&mut machine, 0x61, b"new\r\r").unwrap();
    assert_eq!(tape_read(&mut machine, 0x64), b"old\r\r");

    // Act: Select the same file again and read it, then create a previously missing file.
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    assert_eq!(tape_read(&mut machine, 0x64), b"new\r\r");
    resume_gpib_talker(&mut machine);
    tape_find(&mut machine, b"254").unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    tape_write(&mut machine, 0x61, b"created\r\r").unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    tape_find(&mut machine, b"254").unwrap();

    // Assert: FIND followed by OLD returns the new record.
    assert_eq!(tape_read(&mut machine, 0x64), b"created\r\r");
}

// What: Interrupt a SAVE transfer before its terminating EOI, then FIND and OLD.
// Why: The device must not commit an incomplete program transfer.
#[test]
fn tape_save_does_not_commit_before_eoi() {
    // Arrange: Select an existing program and begin a SAVE transfer.
    let mut machine = Machine::default();
    machine.store_tape_program(1, b"old\r\r").unwrap();
    configure_gpib_talker(&mut machine);
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    send_gpib_command(&mut machine, 0x25);
    send_gpib_command(&mut machine, 0x61);
    machine.write_byte(0x87b2, 0x08).unwrap();
    machine.write_byte(0x87aa, 0).unwrap();

    // Act: Send one byte without EOI, then select the same file again.
    machine.write_byte(0x87b0, b'x').unwrap();
    machine.write_byte(0x87b1, 0x3c).unwrap();
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    tape_find(&mut machine, b"1").unwrap();

    // Assert: OLD still returns the original complete record.
    assert_eq!(tape_read(&mut machine, 0x64), b"old\r\r");
}

// What: Attempt SAVE before FIND and after CLOSE.
// Why: An absent file number cannot be stored as a valid numbered tape record.
#[test]
fn tape_save_requires_a_selected_file_number() {
    // Arrange: Start a fresh tape without a selected file.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);

    // Act: Attempt SAVE on the unselected device.
    let first = tape_write(&mut machine, 0x61, b"no file");

    // Assert: The machine reports the missing selection.
    assert_eq!(first, Err(MachineBusError::TapeFileNotSelected));

    // Act: Select a file, close it, and attempt SAVE again.
    machine.write_byte(0x87b1, 0x34).unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    tape_find(&mut machine, b"1").unwrap();
    machine.write_byte(0x87b2, 0x00).unwrap();
    send_gpib_command(&mut machine, 0x45);
    send_gpib_command(&mut machine, 0x62);
    let after_close = tape_write(&mut machine, 0x61, b"closed");

    // Assert: CLOSE removes the selected file number.
    assert_eq!(after_close, Err(MachineBusError::TapeFileNotSelected));
}

// What: Send an unrelated secondary command to the default RTC.
// Why: The reference ignores it instead of supplying a time response.
#[test]
fn rtc_ignores_unrelated_secondary_command() {
    // Arrange: Select the RTC talk address with an initially empty input buffer.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x42);

    // Act: Dispatch an unrelated command and switch to listening.
    send_gpib_command(&mut machine, 0x64);
    configure_gpib_listener(&mut machine);

    // Assert: The machine receives the empty-input byte with EOI, not RTC data.
    assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
    assert_eq!(machine.read_byte(0x87b0).unwrap(), 0xff);
    assert_eq!(machine.read_byte(0x87aa).unwrap(), 0x1b);
}

// What: Use a supplied RTC calendar value through the complete PIA response handshake.
// Why: Hosts must control sampling without changing formatting, buffered bytes or EOI.
#[test]
fn rtc_uses_injected_clock_and_retains_buffered_response() {
    // Arrange: Install a fixed calendar callback and address the RTC.
    let mut machine = Machine::default();
    machine.set_rtc_clock(|| DateTime::new(2001, 1, 2, 3, 4, 5, 6_999_999));
    machine.reset().unwrap();
    configure_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x42);

    // Act: Dispatch once, then replace the sampler with a failure before receiving bytes.
    send_gpib_command(&mut machine, 0x6d);
    machine.set_rtc_clock(|| DateTime::new(2001, 13, 2, 3, 4, 5, 0));
    configure_gpib_listener(&mut machine);
    let expected = b"02-JAN-1 03:04:05:006";

    // Assert: The retained response is exact and EOI appears only on its last byte.
    for (index, expected_byte) in expected.iter().copied().enumerate() {
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
        assert_eq!(machine.read_byte(0x87b0).unwrap(), expected_byte);
        assert_eq!(
            machine.read_byte(0x87aa).unwrap() & 0x10 != 0,
            index == expected.len() - 1
        );
        machine.write_byte(0x87b2, 0x98).unwrap();
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
        if index != expected.len() - 1 {
            machine.write_byte(0x87b2, 0x18).unwrap();
        }
    }
}

// What: Fail a supplied RTC sampler only when the read command is dispatched.
// Why: Clock errors must be explicit without affecting ignored commands or installing a callback.
#[test]
fn rtc_propagates_injected_clock_failure_at_dispatch() {
    // Arrange: Install a callback returning a reproducible calendar error.
    let mut machine = Machine::default();
    machine.set_rtc_clock(|| DateTime::new(2001, 13, 2, 3, 4, 5, 0));
    configure_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x42);

    // Act: An unrelated command succeeds; a subsequent RTC read invokes the failing sampler.
    send_gpib_command(&mut machine, 0x64);
    machine.write_byte(0x87b0, 0x6d).unwrap();
    let result = machine.write_byte(0x87b1, 0x3c);

    // Assert: The original callback diagnostic reaches the established public bus error.
    let detail = DateTime::new(2001, 13, 2, 3, 4, 5, 0)
        .unwrap_err()
        .to_string();
    assert_eq!(result, Err(MachineBusError::RtcClockUnavailable { detail }));
}

// What: Read the default RTC through the machine's PIA and GPIB handshake.
// Why: The response must reflect the host's local time at dispatch and end with EOI.
#[test]
fn rtc_returns_current_local_time_through_machine_pias() {
    // Arrange: Select the RTC talk address and capture the host timezone and time.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x42);
    let zone = TimeZone::try_system().unwrap();
    let before = Timestamp::try_from(SystemTime::now()).unwrap();

    // Act: Dispatch the read command and receive bytes until the EOI handshake.
    send_gpib_command(&mut machine, 0x6d);
    let after = Timestamp::try_from(SystemTime::now()).unwrap();
    configure_gpib_listener(&mut machine);
    let mut bytes = Vec::new();
    for _ in 0..32 {
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0xc8);
        bytes.push(machine.read_byte(0x87b0).unwrap());
        let eoi = machine.read_byte(0x87aa).unwrap() & 0x10 != 0;
        machine.write_byte(0x87b2, 0x98).unwrap();
        assert_eq!(machine.read_byte(0x87b2).unwrap(), 0x88);
        if eoi {
            break;
        }
        machine.write_byte(0x87b2, 0x18).unwrap();
    }

    // Assert: The timestamp has the reference fields and resolves within the dispatch interval.
    let response = std::str::from_utf8(&bytes).unwrap();
    let (date, time) = response.split_once(' ').unwrap();
    let fields: Vec<_> = date.split('-').collect();
    assert_eq!(fields.len(), 3);
    let months = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let month = months.iter().position(|name| *name == fields[1]).unwrap() as i8 + 1;
    let year_remainder: i16 = fields[2].parse().unwrap();
    let parts: Vec<_> = time.split(':').collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(
        (
            fields[0].len(),
            parts[0].len(),
            parts[1].len(),
            parts[2].len(),
            parts[3].len()
        ),
        (2, 2, 2, 2, 3)
    );
    let possible_years = [
        zone.to_datetime(before).year(),
        zone.to_datetime(after).year(),
    ];
    assert!(
        possible_years.into_iter().any(|year| {
            if year % 100 != year_remainder {
                return false;
            }
            let date = DateTime::new(
                year,
                month,
                fields[0].parse().unwrap(),
                parts[0].parse().unwrap(),
                parts[1].parse().unwrap(),
                parts[2].parse().unwrap(),
                parts[3].parse::<i32>().unwrap() * 1_000_000,
            )
            .unwrap();
            let candidate = zone.to_ambiguous_timestamp(date);
            [candidate.earlier(), candidate.later()]
                .into_iter()
                .flatten()
                .any(|value| {
                    (before.as_millisecond()..=after.as_millisecond())
                        .contains(&value.as_millisecond())
                })
        }),
        "RTC response {response:?} was not within {before}..={after}"
    );
}

// What: Request the RTC with an invalid system timezone in a separate process.
// Why: A timezone lookup failure must reach the machine instead of silently becoming UTC.
#[test]
fn rtc_reports_invalid_timezone_without_utc_fallback() {
    // Arrange: Isolate the TZ override from other parallel tests.
    const CHILD_MARKER: &str = "TEK4051_RTC_INVALID_ZONE_TEST";
    if std::env::var_os(CHILD_MARKER).is_none() {
        // Act: Run this test in a child with a deliberately invalid timezone.
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "rtc_reports_invalid_timezone_without_utc_fallback",
            ])
            .env(CHILD_MARKER, "1")
            .env("TZ", "not-a-valid-tek4051-timezone")
            .output()
            .unwrap();

        // Assert: The isolated test observed and verified the explicit error.
        assert!(
            output.status.success(),
            "child failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    // Act: Dispatch a read against the default RTC.
    let mut machine = Machine::default();
    configure_gpib_talker(&mut machine);
    send_gpib_command(&mut machine, 0x42);
    send_gpib_command(&mut machine, 0x64);
    machine.write_byte(0x87b0, 0x6d).unwrap();
    let result = machine.write_byte(0x87b1, 0x3c);

    // Assert: The clock failure is explicit and includes the underlying diagnostic.
    assert!(matches!(
        result,
        Err(MachineBusError::RtcClockUnavailable { detail }) if !detail.is_empty()
    ));
}
