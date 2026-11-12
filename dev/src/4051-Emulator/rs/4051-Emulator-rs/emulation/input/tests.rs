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
