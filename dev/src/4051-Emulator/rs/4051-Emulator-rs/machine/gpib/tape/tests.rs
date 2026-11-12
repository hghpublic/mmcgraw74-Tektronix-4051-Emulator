use super::{FileType, GpibBuffers, Tape, TapePrintError};

// What: Load a private editor program while a different public file is selected.
// Why: Editor lookup and cleanup must restore the user's selected file, cursor, and writes.
#[test]
fn private_editor_program_shadows_and_restores_user_file() {
    // Arrange: Select public file 7 and advance its record cursor before staging the editor file.
    let mut tape = Tape::default();
    let mut buffers = GpibBuffers::default();
    let public_bytes = b"20 END\r30 END\r";
    let editor_bytes = b"10 PRINT 123\r";
    tape.store_program(7, public_bytes)
        .expect("file 7 should be a valid public tape program");
    buffers.append_output(b'7');
    tape.find(&mut buffers)
        .expect("the public file should be selectable");
    tape.input(&mut buffers);
    let first_public_record: Vec<_> = (0..7).map(|_| buffers.poll().0).collect();
    assert_eq!(first_public_record, b"20 END\r");
    let public_directory = tape.flash_drive_directory();
    assert_eq!(public_directory.len(), 1);
    assert_eq!(public_directory[0].number, 7);
    assert_eq!(tape.stage_editor_program(editor_bytes), 254);
    assert_eq!(tape.flash_drive_directory(), public_directory);

    // Act: FIND and OLD the private overlay, then read the resumed public record.
    for byte in b"254" {
        buffers.append_output(*byte);
    }
    tape.find(&mut buffers)
        .expect("the private editor file should be selectable");
    tape.old(&mut buffers);
    let private_read: Vec<_> = (0..editor_bytes.len()).map(|_| buffers.poll()).collect();

    // Assert: OLD received the private payload and restored the public selection.
    assert_eq!(
        private_read,
        editor_bytes
            .iter()
            .enumerate()
            .map(|(index, &byte)| (byte, index + 1 == editor_bytes.len()))
            .collect::<Vec<_>>()
    );
    tape.input(&mut buffers);
    let resumed_public_record: Vec<_> = (0..7).map(|_| buffers.poll().0).collect();
    assert_eq!(resumed_public_record, b"30 END\r");
    buffers.append_output(b'X');
    tape.print(&mut buffers)
        .expect("the selected user program should remain writable");

    // Assert: The overlay stays out of the directory, and the resumed cursor/write target file 7.
    assert_eq!(
        tape.export_flash_file(7).unwrap().bytes,
        b"20 END\r30 END\rX"
    );
    assert!(tape.export_flash_file(254).is_err());
    assert_eq!(tape.flash_drive_directory()[0].number, 7);

    // Act: Stage again and clear the overlay as a canceled editor sequence would.
    buffers.append_output(b'7');
    tape.find(&mut buffers)
        .expect("the public file should remain selectable");
    tape.input(&mut buffers);
    for _ in 0..7 {
        buffers.poll();
    }
    tape.stage_editor_program(editor_bytes);
    for byte in b"254" {
        buffers.append_output(*byte);
    }
    tape.find(&mut buffers)
        .expect("the private editor file should be selectable before cancellation");
    tape.clear_editor_program();
    tape.input(&mut buffers);
    let canceled_public_record: Vec<_> = (0..7).map(|_| buffers.poll().0).collect();

    // Assert: Cancellation restores the public record cursor and leaves its distinct slot writable.
    assert_eq!(canceled_public_record, b"30 END\r");
    buffers.append_output(b'Y');
    tape.print(&mut buffers)
        .expect("the selected user program should remain writable after cancellation");
    assert_eq!(
        tape.export_flash_file(7).unwrap().bytes,
        b"20 END\r30 END\rXY"
    );
    assert!(tape.export_flash_file(254).is_err());
}

// What: Reject PRINT when its cumulative record cursor cannot fit usize.
// Why: Cursor overflow must report an error without committing a partially updated file.
#[test]
fn print_position_overflow_preserves_stored_bytes() {
    // Arrange: Select an existing file, then place its private cursor at the boundary.
    let mut tape = Tape::default();
    let mut buffers = GpibBuffers::default();
    tape.store_program(1, b"old\r").unwrap();
    buffers.append_output(b'1');
    tape.find(&mut buffers).unwrap();
    tape.read_position = usize::MAX;
    buffers.append_output(b'x');

    // Act: Attempt to append a byte whose cumulative position would overflow.
    let result = tape.print(&mut buffers);

    // Assert: The error is explicit and reselecting the file returns the unchanged contents.
    assert_eq!(result, Err(TapePrintError::PositionOverflow));
    buffers.append_output(b'1');
    tape.find(&mut buffers).unwrap();
    tape.old(&mut buffers);
    assert_eq!(
        (0..4).map(|_| buffers.poll()).collect::<Vec<_>>(),
        [(b'o', false), (b'l', false), (b'd', false), (b'\r', true)]
    );
}

// What: Load binary-program snapshots below, at, and beyond a 256-byte boundary.
// Why: BOLD must append 0xff and spaces, including a whole block for empty or aligned programs.
#[test]
fn bold_padding_matches_reference_block_boundaries() {
    for (length, expected_length) in [(0, 256), (1, 256), (255, 256), (256, 512), (257, 512)] {
        // Arrange: Save and select a binary program with bytes distinct from its padding.
        let mut tape = Tape::default();
        let mut buffers = GpibBuffers::default();
        let program = vec![0x81; length];
        buffers.append_output(b'1');
        tape.find(&mut buffers).unwrap();
        for &byte in &program {
            buffers.append_output(byte);
        }
        tape.save(&mut buffers, FileType::Binary).unwrap();
        buffers.append_output(b'1');
        tape.find(&mut buffers).unwrap();

        // Act: Request BOLD and consume its complete EOI-terminated response.
        tape.bold(&mut buffers);
        let mut actual = Vec::new();
        loop {
            let (byte, eoi) = buffers.poll();
            actual.push(byte);
            if eoi {
                break;
            }
            assert!(actual.len() < expected_length, "BOLD missed final-byte EOI");
        }

        // Assert: The exact block size, unchanged program, marker, spaces, and final drain match.
        assert_eq!(actual.len(), expected_length, "program length {length}");
        assert_eq!(&actual[..length], program);
        assert_eq!(actual[length], 0xff);
        assert!(actual[length + 1..].iter().all(|&byte| byte == b' '));
        assert_eq!(buffers.poll(), (0xff, true));
    }
}
