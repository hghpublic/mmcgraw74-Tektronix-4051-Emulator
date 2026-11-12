use crate::{
    emulation::input::{InputSequence, KeyModifiers, KeyboardState, ManualKeyId},
    machine::{Machine, cpu},
};

// What: Counts sequenced transitions until they are applied to the emulated keyboard.
// Why: The native transport cap must include input that has moved into the sequencer.
#[test]
fn pending_key_transitions_follow_the_sequencer() {
    // Arrange: Start an idle sequence on a machine whose keyboard can accept manual keys.
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    machine.keyboard_release();
    machine.write_byte(0x0100, 0x20).unwrap();
    machine.write_byte(0x0101, 0xfe).unwrap();
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    let first = ManualKeyId(1);
    let second = ManualKeyId(2);

    // Act: Deliver the first press, queue its release and a second press/release pair.
    sequence.press_manual(
        first,
        b'A',
        false,
        KeyModifiers::NONE,
        &mut keyboard,
        &mut machine,
    );
    assert_eq!(sequence.pending_key_transitions(), 0);
    sequence.release_manual(first, KeyModifiers::NONE, &mut keyboard, &mut machine);
    sequence.press_manual(
        second,
        b'B',
        false,
        KeyModifiers::NONE,
        &mut keyboard,
        &mut machine,
    );
    sequence.release_manual(second, KeyModifiers::NONE, &mut keyboard, &mut machine);

    // Assert: The active release and queued press/release are all still pending.
    assert_eq!(sequence.pending_key_transitions(), 3);

    // Act: Cancel the sequence, which releases the machine key and clears queued work.
    sequence.cancel(&mut keyboard, &mut machine);

    // Assert: Canceled transitions no longer consume the outstanding-input budget.
    assert_eq!(sequence.pending_key_transitions(), 0);
}

// What: Preserve literal spaces when a numbered BASIC line enters the sequencer.
// Why: Spaces inside quoted text must reach the ROM as space-key presses.
#[test]
fn queued_basic_program_preserves_spaces_inside_string_literals() {
    // Arrange: Create an idle sequence and a BASIC line with token and quoted-text spaces.
    let mut machine = Machine::default();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();

    // Act: Queue the program and collect the active and pending line-entry key codes.
    sequence
        .queue_basic_program(
            vec![r#"10 PRINT "HELLO WORLD""#.to_owned()],
            &mut keyboard,
            &mut machine,
        )
        .expect("a numbered BASIC line should be queued");
    let mut queued_codes = vec![
        sequence
            .active
            .as_ref()
            .expect("the first script key should be active")
            .press
            .code,
    ];
    queued_codes.extend(sequence.queued.iter().map(|press| press.code));

    // Assert: Both token spacing and the quoted literal's space use the Tek space key.
    assert_eq!(
        queued_codes,
        [
            b'1', b'0', 0x10, b'P', b'R', b'I', b'N', b'T', 0x10, b'2', b'H', b'E', b'L', b'L',
            b'O', 0x10, b'W', b'O', b'R', b'L', b'D', b'2', b'\r',
        ],
    );
}

// What: Begin editor execution with numbered source or a valid tape selection.
// Why: NEW is not accepted at the 4051 BASIC prompt and corrupts following input.
#[test]
fn editor_program_sequences_skip_unsupported_new_command() {
    // Arrange: Create an input sequencer for keyboard and private-file editor runs.
    let mut machine = Machine::default();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();

    // Act: Queue keyboard source and capture the complete first numbered line.
    sequence
        .queue_editor_program(vec!["10 END".to_owned()], &mut keyboard, &mut machine)
        .expect("valid editor source should be queued");
    let first_keyboard_characters: Vec<_> = std::iter::once(
        sequence
            .active
            .as_ref()
            .expect("the first source character should be active")
            .press
            .code,
    )
    .chain(sequence.queued.iter().map(|press| press.code))
    .collect();

    // Assert: Keyboard mode starts with the complete numbered line, including its Return.
    assert_eq!(first_keyboard_characters, b"10\x10END\r");
    assert_eq!(
        sequence
            .pending_program_commands
            .iter()
            .map(|command| command.iter().map(|key| key.code).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [b"RUN\r".to_vec()]
    );
    assert!(!sequence.resume_program_load_if_ready(true, &mut keyboard, &mut machine));

    // Act: Cancel keyboard input, then queue the private-file load path.
    sequence.cancel(&mut keyboard, &mut machine);
    sequence
        .queue_editor_program_load(254, &mut keyboard, &mut machine)
        .expect("the private editor file should be queued");
    let first_file_command_characters: Vec<_> = std::iter::once(
        sequence
            .active
            .as_ref()
            .expect("the first FIND character should be active")
            .press
            .code,
    )
    .chain(sequence.queued.iter().map(|press| press.code))
    .collect();

    // Assert: File mode selects the exact private slot before loading and running it.
    assert_eq!(first_file_command_characters, b"FIND@5:254\r");
    assert_eq!(
        sequence
            .pending_program_commands
            .iter()
            .map(|command| command.iter().map(|key| key.code).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [b"OLD@5:\r".to_vec(), b"RUN\r".to_vec()]
    );
    assert!(!sequence.resume_program_load_if_ready(true, &mut keyboard, &mut machine));
}
