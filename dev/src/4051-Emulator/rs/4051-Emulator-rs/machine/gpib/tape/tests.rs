use super::{FileType, GpibBuffers, Tape, TapePrintError};

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
