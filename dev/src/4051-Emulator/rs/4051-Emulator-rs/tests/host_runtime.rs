use std::time::Duration;

use tek4051_emulator::{
    emulation::CpuReferenceRenderer,
    host_runtime::{HostRuntime, HostRuntimeError, RuntimeEvents, SpeedFactorError},
    machine::{KeyboardModifier, Machine, MachineBusError, PiaId, PiaPort, cpu},
};

fn type_tek_keys(machine: &mut Machine, runtime: &mut HostRuntime, keys: &[u8]) {
    for &key in keys {
        machine.keyboard_press(key);
        runtime
            .advance(machine, Duration::from_millis(100))
            .unwrap();
        machine.keyboard_release();
        runtime
            .advance(machine, Duration::from_millis(100))
            .unwrap();
    }
}

fn call_binary_tape_routine(machine: &mut Machine, runtime: &mut HostRuntime, name: &[u8]) {
    type_tek_keys(machine, runtime, b"CALL\x10");
    // The Tek keyboard produces a quote from Shift+2, not a raw ASCII quote key code.
    machine.set_keyboard_modifier(KeyboardModifier::Shift, true);
    type_tek_keys(machine, runtime, b"2");
    machine.set_keyboard_modifier(KeyboardModifier::Shift, false);
    type_tek_keys(machine, runtime, name);
    machine.set_keyboard_modifier(KeyboardModifier::Shift, true);
    type_tek_keys(machine, runtime, b"2");
    machine.set_keyboard_modifier(KeyboardModifier::Shift, false);
    type_tek_keys(machine, runtime, b",5\r");
}

// What: Enter and run a numbered BASIC program through the machine keyboard.
// Why: This covers the boot, periodic interrupts, command entry, and rendered program output.
#[test]
fn keyboard_entered_basic_program_matches_reference_display() {
    // Arrange: Start the same reset and host-timing path used by the native application.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Enter "10 PRINT 1" followed by Return; 0x10 is the Tek Space key.
    type_tek_keys(&mut machine, &mut runtime, b"10\x10PRINT\x101\r");

    // Assert: The output line is empty before the program runs.
    let output_line = 104 * CpuReferenceRenderer::WIDTH..118 * CpuReferenceRenderer::WIDTH;
    assert!(
        runtime.display(&mut machine).pixels()[output_line.clone()]
            .iter()
            .all(|&pixel| pixel < 240)
    );

    // Act: Run the program and allow the display refresh to settle.
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Assert: The printed "1" and the full stored-pixel image match the web reference.
    let pixels = runtime.display(&mut machine).pixels();
    assert_eq!(
        pixels[output_line]
            .iter()
            .filter(|&&pixel| pixel >= 240)
            .count(),
        40
    );
    let stored_pixel_hash = pixels.iter().fold(0xcbf29ce484222325_u64, |hash, &pixel| {
        (hash ^ u64::from(pixel >= 240)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(stored_pixel_hash, 0x84a1e19a74efd07d);
}

// What: Load a BASIC program through the machine's Loader and execute it through the ROM.
// Why: PIA-level transfers must also work in the real OLD@1: and RUN workflow.
#[test]
fn loader_runs_basic_program_through_rom() {
    // Arrange: Start the ROM and provide an ASCII program to the default Loader.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();
    machine.load_program(b"10 PRINT 1\r\r");

    // Act: Enter OLD@1: and run the loaded program.
    type_tek_keys(&mut machine, &mut runtime, b"OLD@1:\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();
    let output_line = 104 * CpuReferenceRenderer::WIDTH..118 * CpuReferenceRenderer::WIDTH;
    assert!(
        runtime.display(&mut machine).pixels()[output_line.clone()]
            .iter()
            .all(|&pixel| pixel < 240)
    );
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Run the same program by keyboard, whose display is checked against the web above.
    let mut keyboard_machine = Machine::default();
    keyboard_machine.reset().unwrap();
    let mut keyboard_runtime = HostRuntime::start(&mut keyboard_machine);
    keyboard_runtime
        .advance(&mut keyboard_machine, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut keyboard_machine,
        &mut keyboard_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut keyboard_machine, &mut keyboard_runtime, b"RUN\r");
    keyboard_runtime
        .advance(&mut keyboard_machine, Duration::from_secs(2))
        .unwrap();

    // Assert: The output pixels match keyboard entry in position and shape, not just count.
    let loaded_pixels = &runtime.display(&mut machine).pixels()[output_line.clone()];
    let keyboard_pixels = &keyboard_runtime.display(&mut keyboard_machine).pixels()[output_line];
    assert_eq!(
        loaded_pixels.iter().filter(|&&pixel| pixel >= 240).count(),
        40
    );
    assert!(
        loaded_pixels
            .iter()
            .zip(keyboard_pixels)
            .all(|(&loaded, &keyboard)| (loaded >= 240) == (keyboard >= 240))
    );
}

// What: Import a Flash Drive program, then find, load, and run it through the ROM.
// Why: Host-supplied files must reach BASIC through the same FIND@5:, OLD@5:, and RUN path.
#[test]
fn tape_runs_basic_program_through_rom() {
    // Arrange: Start the ROM and import a filename-marked ASCII program without executing commands.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "1      ASCII   PROG [ROM] 12.txt".to_owned(),
            bytes: b"10 PRINT 1\r\r".to_vec(),
        }]),
        [Ok(())]
    );
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Select, load, and run the stored program.
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"OLD@5:\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();
    let output_line = 126 * CpuReferenceRenderer::WIDTH..140 * CpuReferenceRenderer::WIDTH;
    assert!(
        runtime.display(&mut machine).pixels()[output_line.clone()]
            .iter()
            .all(|&pixel| pixel < 240)
    );
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Assert: The output line matches a separately keyboard-entered program.
    let mut keyboard_machine = Machine::default();
    keyboard_machine.reset().unwrap();
    let mut keyboard_runtime = HostRuntime::start(&mut keyboard_machine);
    keyboard_runtime
        .advance(&mut keyboard_machine, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut keyboard_machine,
        &mut keyboard_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut keyboard_machine, &mut keyboard_runtime, b"RUN\r");
    keyboard_runtime
        .advance(&mut keyboard_machine, Duration::from_secs(2))
        .unwrap();
    let loaded = &runtime.display(&mut machine).pixels()[output_line.clone()];
    let keyboard_line = 104 * CpuReferenceRenderer::WIDTH..118 * CpuReferenceRenderer::WIDTH;
    let keyboard = &keyboard_runtime.display(&mut keyboard_machine).pixels()[keyboard_line];
    assert_eq!(loaded.iter().filter(|&&pixel| pixel >= 240).count(), 40);
    assert!(
        loaded
            .iter()
            .zip(keyboard)
            .all(|(&loaded, &keyboard)| (loaded >= 240) == (keyboard >= 240))
    );
}

// What: Read an ASCII number from tape through BASIC INPUT and print the assigned variable.
// Why: Record transfers must work through the real ROM, not only isolated PIA transactions.
#[test]
fn tape_input_reads_an_ascii_number_through_rom() {
    // Arrange: Boot the ROM and store a CR-terminated ASCII number on the default tape.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.store_tape_program(1, b"1\r").unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Select the file, read its record into A, then print A.
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"INPUT@5:A\r");
    type_tek_keys(&mut machine, &mut runtime, b"PRINT\x10A\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Obtain the known "1" glyph from the keyboard-entered BASIC regression.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut reference,
        &mut reference_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();

    // Assert: INPUT assigned the tape value; PRINT has the reference glyph and horizontal position.
    let width = CpuReferenceRenderer::WIDTH;
    let actual = &runtime.display(&mut machine).pixels()[126 * width..140 * width];
    let expected = &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert_eq!(actual.iter().filter(|&&pixel| pixel >= 240).count(), 40);
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
}

// What: Write a number to tape with BASIC PRINT, reselect it, and read it with INPUT.
// Why: ASCII tape output must round-trip through the ROM and appear as the expected variable value.
#[test]
fn tape_print_and_input_round_trip_a_number_through_rom() {
    // Arrange: Boot the ROM with an empty default tape.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Create file 1 by PRINT, refresh its snapshot, read into A, then print A on screen.
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"PRINT@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"INPUT@5:A\r");
    type_tek_keys(&mut machine, &mut runtime, b"PRINT\x10A\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Obtain the known "1" output from the keyboard-entered BASIC regression.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut reference,
        &mut reference_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();

    // Assert: The tape round-trip produces the reference glyph at the expected output line.
    let width = CpuReferenceRenderer::WIDTH;
    let actual = &runtime.display(&mut machine).pixels()[170 * width..184 * width];
    let expected = &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert_eq!(actual.iter().filter(|&&pixel| pixel >= 240).count(), 40);
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
}

// What: Write a binary number, reselect its tape file, and READ it into a BASIC variable.
// Why: The byte-by-byte WRITE and one-shot READ paths must round-trip through the real ROM.
#[test]
fn tape_write_and_read_round_trip_a_number_through_rom() {
    // Arrange: Boot the ROM with an empty default tape.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Write the number 1 to file 2, FIND it again, READ into A, and print A.
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:2\r");
    type_tek_keys(&mut machine, &mut runtime, b"WRITE@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:2\r");
    type_tek_keys(&mut machine, &mut runtime, b"READ@5:A\r");
    type_tek_keys(&mut machine, &mut runtime, b"PRINT\x10A\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Obtain the known "1" glyph from the keyboard-entered BASIC regression.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut reference,
        &mut reference_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();

    // Assert: The binary round-trip produces the reference glyph at the expected output line.
    let width = CpuReferenceRenderer::WIDTH;
    let actual = &runtime.display(&mut machine).pixels()[170 * width..184 * width];
    let expected = &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert_eq!(actual.iter().filter(|&&pixel| pixel >= 240).count(), 40);
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
}

// What: Save a BASIC program to tape, change it, reload it, and run it.
// Why: SAVE must persist the actual ROM output so a later FIND and OLD recover the program.
#[test]
fn tape_save_and_reload_basic_program_through_rom() {
    // Arrange: Start the ROM and enter the program to save.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(&mut machine, &mut runtime, b"10\x10PRINT\x101\r");

    // Act: Save it, replace the line in memory, then FIND and OLD the tape copy.
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"SAVE@5:\r");
    type_tek_keys(&mut machine, &mut runtime, b"10\x10PRINT\x102\r");
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:1\r");
    type_tek_keys(&mut machine, &mut runtime, b"OLD@5:\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();
    let before_run = runtime.display(&mut machine).pixels().to_vec();
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Obtain the known "1" output mask from the keyboard-entry reference.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut reference,
        &mut reference_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();
    let width = CpuReferenceRenderer::WIDTH;
    let reference_line =
        &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert_eq!(
        reference_line.iter().filter(|&&pixel| pixel >= 240).count(),
        40
    );

    // Assert: RUN adds the reference "1" glyph on a previously blank line, not "2".
    let after_run = runtime.display(&mut machine).pixels();
    assert!(
        (0..=CpuReferenceRenderer::HEIGHT - 14).any(|row| {
            let range = row * width..(row + 14) * width;
            before_run[range.clone()].iter().all(|&pixel| pixel < 240)
                && after_run[range]
                    .iter()
                    .zip(reference_line)
                    .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
        }),
        "the saved program did not print the expected output"
    );
}

// What: BSAVE a BASIC program, modify it, then reload with BOLD and run the saved version.
// Why: Binary-program transfers and padding must work through the ROM, not just byte-level tests.
#[test]
fn tape_bsave_and_bold_reload_a_basic_program_through_rom() {
    // Arrange: Boot the ROM and enter the program to save.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(&mut machine, &mut runtime, b"10\x10PRINT\x101\r");

    // Act: Save binary program bytes, replace the line, then FIND and BOLD the saved copy.
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:3\r");
    call_binary_tape_routine(&mut machine, &mut runtime, b"BSAVE");
    type_tek_keys(&mut machine, &mut runtime, b"10\x10PRINT\x102\r");
    type_tek_keys(&mut machine, &mut runtime, b"FIND@5:3\r");
    call_binary_tape_routine(&mut machine, &mut runtime, b"BOLD");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();
    let before_run = runtime.display(&mut machine).pixels().to_vec();
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Obtain the known "1" glyph from the keyboard-entered BASIC regression.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut reference,
        &mut reference_runtime,
        b"10\x10PRINT\x101\r",
    );
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();
    let width = CpuReferenceRenderer::WIDTH;
    let expected = &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert_eq!(expected.iter().filter(|&&pixel| pixel >= 240).count(), 40);

    // Assert: RUN draws the saved program's "1" on a newly populated line, not the replacement "2".
    let after_run = runtime.display(&mut machine).pixels();
    assert!(
        (0..=CpuReferenceRenderer::HEIGHT - 14).any(|row| {
            let range = row * width..(row + 14) * width;
            before_run[range.clone()].iter().all(|&pixel| pixel < 240)
                && after_run[range]
                    .iter()
                    .zip(expected)
                    .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
        }),
        "the binary-saved program did not print the expected output"
    );
}

// What: Read a tape LIST entry through secondary address 19 into a BASIC string.
// Why: Fixed-width metadata must be usable by the real ROM's DIM, INPUT, and LEN workflow.
#[test]
fn tape_list_metadata_is_readable_through_rom() {
    // Arrange: Boot the ROM with a tape program and a Loader program that reads its metadata.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.store_tape_program(1, b"10 END\r\r").unwrap();
    machine.load_program(b"10 DIM A$(60)\r20 FIND@5:1\r30 INPUT@5,19:A$\r40 PRINT LEN(A$)\r\r");
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Load and run the metadata reader without bypassing ROM command parsing.
    type_tek_keys(&mut machine, &mut runtime, b"OLD@1:\r");
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Obtain the expected "38" glyphs from a separately entered BASIC program.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(
        &mut reference,
        &mut reference_runtime,
        b"10\x10PRINT\x1038\r",
    );
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();

    // Assert: LEN observed all 38 header bytes, with the expected complete output glyph mask.
    let width = CpuReferenceRenderer::WIDTH;
    let actual = &runtime.display(&mut machine).pixels()[104 * width..118 * width];
    let expected = &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert!(expected.iter().any(|&pixel| pixel >= 240));
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
}

// What: Write and reread a tape header through BASIC's PRINT/INPUT secondary address 19.
// Why: ROM parsing, EOI termination, renumbering, description extraction, and full metadata must agree.
#[test]
fn tape_list_header_round_trip_runs_through_rom() {
    // Arrange: Boot with a stored program and a Loader program that writes then prints its new entry.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.store_tape_program(1, b"10 PRINT 1\r\r").unwrap();
    machine.load_program(
        b"10 DIM H$(60)\r20 DIM A$(60)\r30 H$=\"2      ASCII   TEXT [ROM] 999\"\r\
          40 FIND@5:1\r50 PRINT@5,19:H$\r60 FIND@5:2\r70 INPUT@5,19:A$\r80 PRINT A$\r\r",
    );
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_secs(20))
        .unwrap();

    // Act: Execute the header transfer and metadata read without bypassing the ROM.
    type_tek_keys(&mut machine, &mut runtime, b"OLD@1:\r");
    type_tek_keys(&mut machine, &mut runtime, b"RUN\r");
    runtime
        .advance(&mut machine, Duration::from_secs(2))
        .unwrap();

    // Arrange: Render the independently specified complete entry as a BASIC string literal.
    let mut reference = Machine::default();
    reference.reset().unwrap();
    reference.load_program(b"10 PRINT \"2      ASCII   TEXT ROM              12\"\r\r");
    let mut reference_runtime = HostRuntime::start(&mut reference);
    reference_runtime
        .advance(&mut reference, Duration::from_secs(20))
        .unwrap();
    type_tek_keys(&mut reference, &mut reference_runtime, b"OLD@1:\r");
    type_tek_keys(&mut reference, &mut reference_runtime, b"RUN\r");
    reference_runtime
        .advance(&mut reference, Duration::from_secs(2))
        .unwrap();

    // Assert: The complete output matches number, labels, description, padding, and actual size.
    let width = CpuReferenceRenderer::WIDTH;
    let actual = &runtime.display(&mut machine).pixels()[104 * width..118 * width];
    let expected = &reference_runtime.display(&mut reference).pixels()[104 * width..118 * width];
    assert!(expected.iter().any(|&pixel| pixel >= 240));
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
}

// What: Confirms execution, keyboard refresh, and display updates follow the reference cadence.
// Why: Host frames must not run machine callbacks before their elapsed-time deadlines.
#[test]
fn host_intervals_drive_machine_and_display_callbacks() {
    // Arrange: Start from the ROM's display wait; runtime start releases it.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.run_steps(100_000).unwrap();
    assert_eq!(machine.registers().pc, 0xc6b4);
    let mut runtime = HostRuntime::start(&mut machine);

    // Act: Advance to just before the first 10 ms CPU and keyboard callbacks.
    let before_deadline = runtime
        .advance(&mut machine, Duration::from_millis(9))
        .unwrap();

    // Assert: No CPU, keyboard, or display callback has fired early.
    assert_eq!(machine.registers().pc, 0xc6b4);
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
    assert_eq!(before_deadline.display_refreshes, 0);

    // Act: Reach the 10 ms deadline, then advance to the 100 ms display boundary.
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();
    assert_ne!(machine.registers().pc, 0xc6b4);
    assert_ne!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);

    // Act: Advance to 99 ms, then reach the display refresh deadline.
    let before_display_deadline = runtime
        .advance(&mut machine, Duration::from_millis(89))
        .unwrap();
    let at_display_deadline = runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: The display callback is due only at 100 ms.
    assert_eq!(before_display_deadline.display_refreshes, 0);
    assert_eq!(at_display_deadline.display_refreshes, 1);

    // Assert: Keyboard refresh remains latched after the display boundary.
    assert_ne!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
}

// What: Pause execution across elapsed wall time and then resume at a fresh callback boundary.
// Why: Paused time must not run CPU or display callbacks or shorten the next interval.
#[test]
fn pause_freezes_callbacks_and_resume_restarts_the_interval() {
    // Arrange: Use RAM NOPs so the CPU position reveals any executed callback.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..2_265 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    machine.write_byte(0x87a9, 0x04).unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_millis(5))
        .unwrap();
    runtime.pause();

    // Act: Advance one paused second, resume, and test both sides of a full 10 ms interval.
    let paused_events = runtime
        .advance(&mut machine, Duration::from_secs(1))
        .unwrap();
    assert_eq!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
    runtime.resume().unwrap();
    runtime
        .advance(&mut machine, Duration::from_millis(9))
        .unwrap();
    let before_deadline = machine.registers().pc;
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: Paused time ran no callbacks, and resumed execution waited 10 ms.
    assert_eq!(paused_events.display_refreshes, 0);
    assert_eq!(before_deadline, program_start);
    assert_eq!(machine.registers().pc, program_start + 2_264);
    assert_ne!(machine.read_byte(0x87a9).unwrap() & 0x80, 0);
}

// What: Confirms the speed factor scales the CPU instruction budget per 10 ms interval.
// Why: The reference exposes a speed multiplier independently of its callback cadence.
#[test]
fn speed_factor_scales_instructions_per_execution_interval() {
    // Arrange: Fill RAM with one-byte NOPs so the PC records the executed instruction count.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..4_529 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.set_speed_factor(2.0).unwrap();

    // Act: Advance through one 10 ms execution callback.
    runtime
        .advance(&mut machine, Duration::from_millis(10))
        .unwrap();

    // Assert: The source formula yields a 4,528-instruction budget at speed factor 2.
    assert_eq!(machine.registers().pc, program_start + 4_528);
}

// What: Confirms a display erase remains busy for 500 ms before host completion.
// Why: The CPU-visible display-ready signal must follow the reference's erase delay.
#[test]
fn display_erase_completes_after_five_hundred_milliseconds() {
    // Arrange: Run a masked branch loop and trigger the display's erase handshake.
    let mut machine = Machine::default();
    machine.write_byte(0x0100, 0x20).unwrap();
    machine.write_byte(0x0101, 0xfe).unwrap();
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    machine.set_pia_input(PiaId::DisplayX, PiaPort::A, 0xa0);
    let mut runtime = HostRuntime::start(&mut machine);

    machine.write_byte(0x8794, 0x10).unwrap();
    machine.write_byte(0x8794, 0).unwrap();
    machine.write_byte(0x8795, 0x04).unwrap();
    machine.write_byte(0x8794, 0x10).unwrap();
    machine.write_byte(0x8797, 0x04).unwrap();
    machine.write_byte(0x8796, 0x10).unwrap();
    machine.write_byte(0x8796, 0).unwrap();
    machine.write_byte(0x8794, 0).unwrap();
    assert!(machine.display_erase_pending());
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(0, 0),
        Some(u8::MAX)
    );
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(
            CpuReferenceRenderer::WIDTH / 2,
            CpuReferenceRenderer::HEIGHT / 2
        ),
        Some(u8::MAX)
    );
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(
            CpuReferenceRenderer::WIDTH - 1,
            CpuReferenceRenderer::HEIGHT - 1
        ),
        Some(u8::MAX)
    );

    // Act: Pause during erase and advance wall time to the original 499 ms boundary.
    runtime
        .advance(&mut machine, Duration::from_millis(200))
        .unwrap();
    runtime.pause();
    let paused_pc = machine.registers().pc;
    let before_completion = runtime
        .advance(&mut machine, Duration::from_millis(299))
        .unwrap();
    assert!(machine.display_erase_pending());
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(0, 0),
        Some(u8::MAX)
    );
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(
            CpuReferenceRenderer::WIDTH / 2,
            CpuReferenceRenderer::HEIGHT / 2
        ),
        Some(u8::MAX)
    );
    let revision_before_completion = runtime.display(&mut machine).revision();
    let at_completion = runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: Pause does not delay readiness or run CPU, keyboard, or phosphor callbacks.
    assert!(!machine.display_erase_pending());
    assert!(runtime.display(&mut machine).revision() > revision_before_completion);
    assert_eq!(runtime.display(&mut machine).pixel_intensity(0, 0), Some(0));
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(
            CpuReferenceRenderer::WIDTH / 2,
            CpuReferenceRenderer::HEIGHT / 2
        ),
        Some(0)
    );
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(
            CpuReferenceRenderer::WIDTH - 1,
            CpuReferenceRenderer::HEIGHT - 1
        ),
        Some(0)
    );
    assert_eq!(before_completion.display_refreshes, 0);
    assert_eq!(at_completion.display_refreshes, 0);
    assert_eq!(machine.registers().pc, paused_pc);
}

// What: Complete two staggered erase requests across a runtime restart.
// Why: Every web ERASE timeout survives reset and can blank graphics drawn after an earlier timeout.
#[test]
fn repeated_erases_keep_independent_deadlines_across_restart() {
    // Arrange: Pause a machine with one erase, then request another 200 ms later.
    let mut machine = Machine::default();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.pause();
    machine.erase_display();
    runtime
        .advance(&mut machine, Duration::from_millis(200))
        .unwrap();
    machine.erase_display();

    // Act: Restart as a native Reset does, then pause and reach the first deadline.
    machine.reset().unwrap();
    runtime.restart(&mut machine);
    runtime.pause();
    runtime
        .advance(&mut machine, Duration::from_millis(299))
        .unwrap();

    // Assert: Both requests retain the flash until the first original deadline.
    assert!(machine.display_erase_pending());
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(21, 799),
        Some(255)
    );
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();
    assert!(!machine.display_erase_pending());
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(21, 799),
        Some(0)
    );

    // Arrange: Plot a stored vector after the first callback has signaled readiness.
    for (port, control, value) in [
        (0x878c, 0x878d, 0),
        (0x878e, 0x878f, 20),
        (0x8794, 0x8795, 0x80),
        (0x8796, 0x8797, 5),
    ] {
        machine.write_byte(port, 0xff).unwrap();
        machine.write_byte(control, 0x04).unwrap();
        machine.write_byte(port, value).unwrap();
    }
    machine.write_byte(0x8796, 10).unwrap();

    // Act: Advance to one millisecond before the second independent deadline.
    runtime
        .advance(&mut machine, Duration::from_millis(199))
        .unwrap();

    // Assert: Stored graphics persist until the second callback, then become black.
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(21, 799),
        Some(240)
    );
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(21, 799),
        Some(0)
    );
    assert!(!machine.display_erase_pending());
}

// What: Stop CPU execution while an erase timeout is pending.
// Why: Independent web timeouts still complete even if an execution callback fails.
#[test]
fn erase_deadline_survives_execution_error_and_stopped_advances() {
    // Arrange: Request erase before a CPU callback that reads an unsupported ROM bank.
    let mut machine = Machine::default();
    let mut runtime = HostRuntime::start(&mut machine);
    machine.erase_display();
    machine.write_byte(0x87c0, 0x08).unwrap();
    machine.cpu_mut().registers.pc = 0x9800;

    // Act: Advance 499 ms; execution fails at 10 ms but erase time still advances.
    let failure = runtime.advance(&mut machine, Duration::from_millis(499));

    // Assert: The explicit machine error is preserved and the erase remains busy.
    assert!(matches!(failure, Err(HostRuntimeError::Machine { .. })));
    assert!(machine.display_erase_pending());
    assert_eq!(
        runtime.display(&mut machine).pixel_intensity(0, 0),
        Some(255)
    );
    let stopped_pc = machine.registers().pc;

    // Act: Advance the last millisecond while the runtime reports that execution stopped.
    let stopped = runtime.advance(&mut machine, Duration::from_millis(1));

    // Assert: Readiness and blanking occur without restarting CPU execution.
    assert_eq!(stopped, Err(HostRuntimeError::Stopped));
    assert_eq!(machine.registers().pc, stopped_pc);
    assert!(!machine.display_erase_pending());
    assert_eq!(runtime.display(&mut machine).pixel_intensity(0, 0), Some(0));
}

// What: Confirms rejected speed factors do not replace a previously selected instruction budget.
// Why: A failed setting change must preserve the last valid emulation speed.
#[test]
fn invalid_speed_factor_preserves_the_active_instruction_budget() {
    // Arrange: Select x2 speed and place enough one-byte NOPs in RAM to observe its budget.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..2_265 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.set_speed_factor(2.0).unwrap();

    // Act: Reject zero, non-finite, and overflowing speed factors.
    assert_eq!(
        runtime.set_speed_factor(0.0),
        Err(SpeedFactorError::NotFiniteAndPositive)
    );
    assert_eq!(
        runtime.set_speed_factor(f64::NAN),
        Err(SpeedFactorError::NotFiniteAndPositive)
    );
    assert_eq!(
        runtime.set_speed_factor(f64::MAX),
        Err(SpeedFactorError::InstructionBudgetOverflow)
    );

    // Act: Execute one 10 ms interval after the rejected changes.
    runtime
        .advance(&mut machine, Duration::from_millis(10))
        .unwrap();

    // Assert: The valid x2 budget remains 4,528 one-byte instructions.
    assert_eq!(machine.registers().pc, program_start + 4_528);
}

// What: Confirms a machine error stops timing, including at a shared 100 ms boundary.
// Why: Failed CPU callbacks must not leave elapsed counters due and cause a later zero-step loop.
#[test]
fn machine_error_stops_runtime_at_a_callback_boundary() {
    // Arrange: Advance to 90 ms in RAM, then select an unsupported bank at the 100 ms boundary.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..20_377 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = program_start;
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .advance(&mut machine, Duration::from_millis(90))
        .unwrap();
    machine.write_byte(0x87c0, 0x08).unwrap();
    machine.cpu_mut().registers.pc = 0x9800;

    // Act: The next CPU callback fails on the same boundary as display refresh.
    let result = runtime.advance(&mut machine, Duration::from_millis(10));

    // Assert: The bus error is returned and this instance refuses to resume.
    assert_eq!(
        result,
        Err(HostRuntimeError::Machine {
            source: cpu::CpuError::Bus(MachineBusError::UnsupportedRomBank {
                selector: 1,
                address: 0x9800,
            }),
            events: RuntimeEvents {
                display_refreshes: 1,
            },
        })
    );
    assert_eq!(
        runtime.advance(&mut machine, Duration::from_millis(1)),
        Err(HostRuntimeError::Stopped)
    );
    assert_eq!(runtime.resume(), Err(HostRuntimeError::Stopped));
}

// What: Restart host timing after a failed callback and machine reset.
// Why: Recovery must retain speed while honoring the reference erase delay.
#[test]
fn runtime_restart_preserves_speed_and_completes_reset_erase() {
    // Arrange: Stop an x2 runtime on an unsupported ROM access.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.set_speed_factor(2.0).unwrap();
    machine.write_byte(0x87c0, 0x08).unwrap();
    machine.cpu_mut().registers.pc = 0x9800;
    assert!(
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .is_err()
    );

    // Arrange: Reset, start the display erase, and place a stable branch after 4,528 NOPs.
    machine.reset().unwrap();
    let program_start = 0x0100;
    for offset in 0..4_528 {
        machine.write_byte(program_start + offset, 0x01).unwrap();
    }
    machine.write_byte(program_start + 4_528, 0x20).unwrap();
    machine.write_byte(program_start + 4_529, 0xfe).unwrap();
    machine.cpu_mut().registers.pc = program_start;
    machine.erase_display();
    runtime.restart(&mut machine);

    // Act: Run through one CPU interval and the 500 ms erase deadline.
    runtime
        .advance(&mut machine, Duration::from_millis(10))
        .unwrap();
    assert_eq!(machine.registers().pc, program_start + 4_528);
    runtime
        .advance(&mut machine, Duration::from_millis(489))
        .unwrap();
    assert!(machine.display_erase_pending());
    runtime
        .advance(&mut machine, Duration::from_millis(1))
        .unwrap();

    // Assert: The retained x2 budget ran, then the pending erase cleared at 500 ms.
    assert!(!machine.display_erase_pending());
    assert_eq!(runtime.display(&mut machine).pixel_intensity(0, 0), Some(0));
}
