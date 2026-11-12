use std::io::{Cursor, Read, Write};

use super::{ArchiveFileRef, FileType, FileUsage, read_archive, write_archive};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

// What: Stream borrowed files into exact header/content pairs and decode them without a machine.
// Why: The shared ZIP codec must preserve raw payloads and lossless metadata independently of tape state.
#[test]
fn archive_streams_exact_pairs_and_decodes_metadata() {
    // Arrange: Include binary bytes, isolated UTF-16/control units and an empty LAST entry.
    let description = [0xd800, 0x22, 0];
    let payload = [0, 0xff];
    let files = [
        ArchiveFileRef {
            number: 1,
            file_type: Some(FileType::Ascii),
            usage: Some(FileUsage::Program),
            description: &description,
            bytes: &payload,
        },
        ArchiveFileRef {
            number: 254,
            file_type: Some(FileType::Last),
            usage: None,
            description: &[],
            bytes: &[],
        },
    ];

    // Act: Export to an owned sink, inspect the ZIP and decode the same stream independently.
    let bytes = write_archive(Cursor::new(Vec::new()), files.into_iter())
        .unwrap()
        .into_inner();
    let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let decoded = read_archive(Cursor::new(&bytes)).unwrap();

    // Assert: Exact ordered entry names/data and decoded fields preserve the web archive shape.
    let expected: [(&str, &[u8]); 4] = [
        ("0001", br#"[1,"A","P","\ud800\"\u0000"]"#),
        ("0002", &payload),
        ("0003", br#"[254,"L","",""]"#),
        ("0004", &[]),
    ];
    assert_eq!(zip.len(), expected.len());
    for (index, (name, expected_bytes)) in expected.into_iter().enumerate() {
        let mut entry = zip.by_index(index).unwrap();
        let mut actual = Vec::new();
        entry.read_to_end(&mut actual).unwrap();
        assert_eq!(entry.name(), name);
        assert_eq!(actual, expected_bytes);
    }
    assert_eq!(decoded.keys().copied().collect::<Vec<_>>(), [1, 254]);
    assert_eq!(decoded[&1].header.number, 1);
    assert_eq!(decoded[&1].header.file_type, FileType::Ascii);
    assert_eq!(decoded[&1].header.usage, Some(FileUsage::Program));
    assert_eq!(decoded[&1].header.description, description);
    assert_eq!(decoded[&1].bytes, payload);
    assert_eq!(decoded[&254].header.file_type, FileType::Last);
    assert_eq!(decoded[&254].header.usage, None);
    assert!(decoded[&254].header.description.is_empty());
    assert!(decoded[&254].bytes.is_empty());
}

// What: Report an empty export, a failed output stream and a later malformed archive header.
// Why: Interchange failures must not become successful empty output or a partially decoded store.
#[test]
fn archive_stream_failures_remain_explicit() {
    // Arrange: An empty iterator has no stored files to export.
    let files = std::iter::empty();

    // Act: Try exporting without creating a tape object.
    let error = write_archive(Cursor::new(Vec::<u8>::new()), files).unwrap_err();

    // Assert: The original empty-store diagnostic is retained.
    assert_eq!(error, "Flash Drive contains no files");

    // Arrange: A fixed zero-byte sink cannot accept the first ZIP header.
    let mut storage = [];
    let file = ArchiveFileRef {
        number: 1,
        file_type: Some(FileType::Ascii),
        usage: Some(FileUsage::Program),
        description: &[],
        bytes: b"payload",
    };

    // Act: Stream a valid entry into the unavailable output.
    let error =
        write_archive(Cursor::new(storage.as_mut_slice()), std::iter::once(file)).unwrap_err();

    // Assert: The stream failure identifies the entry instead of returning a completed ZIP.
    assert!(error.contains("cannot create ZIP entry 0001"), "{error}");

    // Arrange: A valid first pair precedes malformed metadata; names are unrelated to file numbers.
    let entries: [(&str, &[u8]); 4] = [
        ("first-header", br#"[2,"A","P","valid"]"#),
        ("first-content", b"first"),
        ("later-header", br#"[1,"A","P","bad",true]"#),
        ("later-content", b"ignored"),
    ];
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    let source = zip.finish().unwrap();

    // Act: Decode the complete archive, not only its valid first pair.
    let error = read_archive(source).unwrap_err();

    // Assert: Later schema failure rejects the result instead of returning the first file.
    assert!(error.contains("archive header expected"), "{error}");
}
