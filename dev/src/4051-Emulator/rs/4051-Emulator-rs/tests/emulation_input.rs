use std::time::Duration;

use tek4051_emulator::{
    emulation_input::{
        InputAction, InputFrame, InputSequence, KeyModifiers, KeyboardState, ManualKeyId,
    },
    host_runtime::HostRuntime,
    machine::{Machine, cpu},
};

fn running_keyboard() -> (Machine, HostRuntime) {
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    machine.keyboard_release();
    machine.write_byte(0x0100, 0x20).unwrap();
    machine.write_byte(0x0101, 0xfe).unwrap();
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    let runtime = HostRuntime::start(&mut machine);
    (machine, runtime)
}

// What: Deliver two short, translated panel taps without native events or widgets.
// Why: The extracted sequencer must preserve release-time modifiers and 50/30 ms key phases.
#[test]
fn neutral_taps_keep_modifiers_and_press_release_boundaries() {
    // Arrange: Queue a shifted/control tap followed by an unmodified tap on a running board.
    let (mut machine, mut runtime) = running_keyboard();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    let held = KeyModifiers {
        shift: true,
        control: true,
    };
    sequence.press(0x60, held, &mut keyboard, &mut machine);
    sequence.release(held, &mut keyboard, &mut machine);
    sequence.press(0x61, KeyModifiers::NONE, &mut keyboard, &mut machine);
    sequence.release(KeyModifiers::NONE, &mut keyboard, &mut machine);

    // Act: Reconcile released host modifiers and advance to just before the first release.
    keyboard.apply_modifiers(&mut machine, KeyModifiers::NONE);
    sequence
        .advance(
            Duration::from_millis(49),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();

    // Assert: Release-time modifiers and the first key remain visible through 49 ms.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xe0);
    assert_eq!(machine.read_byte(0x87aa).unwrap() & 3, 0);

    // Act: Complete the press, then advance to just before the next permitted press.
    sequence
        .advance(
            Duration::from_millis(1),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    sequence
        .advance(
            Duration::from_millis(29),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    sequence
        .advance(
            Duration::from_millis(1),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();

    // Assert: The second tap starts only after a full released interval, without inherited modifiers.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xe1);
    assert_eq!(machine.read_byte(0x87aa).unwrap() & 3, 3);
}

// What: Preserve two taps that reuse one browser key identity before the machine advances.
// Why: The later keyup must release its own queued press, not the earlier active press.
#[test]
fn rapid_manual_taps_are_queued_in_order() {
    // Arrange: Prepare a running machine and two already-released taps sharing one host identity.
    let (mut machine, mut runtime) = running_keyboard();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    let first = ManualKeyId(1);
    let second = first;
    let frame = InputFrame {
        actions: vec![
            InputAction::SequencedManualPress {
                key: first,
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualRelease(first),
            InputAction::SequencedManualPress {
                key: second,
                code: b'B',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualRelease(second),
        ],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };

    // Act: Deliver both taps in one frame without giving the emulated CPU an intervening tick.
    frame.apply(&mut sequence, &mut keyboard, &mut machine);

    // Assert: The first key remains on the keyboard port while the second tap waits in order.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'A' | 0x80);
    sequence
        .advance(
            Duration::from_millis(49),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'A' | 0x80);

    // Act: Complete the minimum hold and release interval before advancing the queued tap.
    sequence
        .advance(
            Duration::from_millis(1),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    sequence
        .advance(
            Duration::from_millis(29),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    sequence
        .advance(
            Duration::from_millis(1),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();

    // Assert: The second translated code is presented only after the first was released.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'B' | 0x80);
    sequence
        .advance(
            Duration::from_millis(80),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Preserve distinct manual keys whose physical key-down intervals overlap.
// Why: Keyboard rollover can deliver a new key-down before the previous key-up, and neither tap may be lost.
#[test]
fn overlapping_manual_keydowns_are_queued_in_order() {
    // Arrange: Prepare two translated keys that overlap before either is released.
    let (mut machine, mut runtime) = running_keyboard();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    let first = ManualKeyId(1);
    let second = ManualKeyId(2);
    let press_frame = InputFrame {
        actions: vec![
            InputAction::SequencedManualPress {
                key: first,
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualPress {
                key: second,
                code: b'B',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
        ],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };
    let release_second = InputFrame {
        actions: vec![InputAction::SequencedManualRelease(second)],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };
    let release_first = InputFrame {
        actions: vec![InputAction::SequencedManualRelease(first)],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };

    // Act: Deliver both key-downs before either matching key-up.
    press_frame.apply(&mut sequence, &mut keyboard, &mut machine);

    // Assert: The first key is active and the second remains queued rather than being dropped.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'A' | 0x80);

    // Act: Release the queued key first and let the active manual-key interval expire.
    release_second.apply(&mut sequence, &mut keyboard, &mut machine);
    sequence
        .advance(
            Duration::from_millis(49),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();

    // Assert: Releasing B does not release A, even though A is the active press.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'A' | 0x80);

    // Act: Release A and complete its release interval before presenting B.
    release_first.apply(&mut sequence, &mut keyboard, &mut machine);
    sequence
        .advance(
            Duration::from_millis(31),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();

    // Assert: B appears only after A's matching release and remains visible for its press interval.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'B' | 0x80);
    sequence
        .advance(
            Duration::from_millis(80),
            &mut keyboard,
            &mut machine,
            &mut runtime,
            KeyModifiers::NONE,
        )
        .unwrap();
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Queue a burst of distinct manual taps before advancing the emulated keyboard.
// Why: Fast host event streams must be serialized without dropping or reordering translated codes.
#[test]
fn rapid_manual_tap_burst_reaches_the_keyboard_in_order() {
    // Arrange: Prepare twelve released taps with unique identities and increasing key codes.
    let (mut machine, mut runtime) = running_keyboard();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    let codes = (b'A'..=b'L').collect::<Vec<_>>();
    let actions = codes
        .iter()
        .enumerate()
        .flat_map(|(index, &code)| {
            let key = ManualKeyId(index as u64);
            [
                InputAction::SequencedManualPress {
                    key,
                    code,
                    invert_shift: false,
                    modifiers: KeyModifiers::NONE,
                },
                InputAction::SequencedManualRelease(key),
            ]
        })
        .collect();
    let frame = InputFrame {
        actions,
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };

    // Act: Submit the burst at once, then advance one 50/30 ms tap interval per key.
    frame.apply(&mut sequence, &mut keyboard, &mut machine);
    for (index, &code) in codes.iter().enumerate() {
        assert_eq!(machine.read_byte(0x87a8).unwrap(), code | 0x80);
        sequence
            .advance(
                Duration::from_millis(80),
                &mut keyboard,
                &mut machine,
                &mut runtime,
                KeyModifiers::NONE,
            )
            .unwrap();
        if let Some(&next_code) = codes.get(index + 1) {
            assert_eq!(machine.read_byte(0x87a8).unwrap(), next_code | 0x80);
        }
    }

    // Assert: The final tap also releases, leaving no key asserted on the keyboard port.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
}

// What: Cancellation removes both an active manual tap and its queued successor.
// Why: Focus loss, reset, and runtime failure must not replay delayed browser input.
#[test]
fn cancel_discards_active_and_queued_manual_taps() {
    // Arrange: Queue two released taps while the first key is visible on the keyboard port.
    let (mut machine, mut runtime) = running_keyboard();
    runtime.pause();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    let frame = InputFrame {
        actions: vec![
            InputAction::SequencedManualPress {
                key: ManualKeyId(1),
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualRelease(ManualKeyId(1)),
            InputAction::SequencedManualPress {
                key: ManualKeyId(2),
                code: b'B',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualRelease(ManualKeyId(2)),
        ],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };
    frame.apply(&mut sequence, &mut keyboard, &mut machine);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'A' | 0x80);

    // Act: Cancel the sequence to release the active key and discard its queued successor.
    sequence.cancel(&mut keyboard, &mut machine);

    // Assert: Cancellation releases the visible key and modifier lines immediately.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(machine.read_byte(0x87aa).unwrap() & 3, 3);

    // Act: Sample 10 ms boundaries through the time the queued tap would have occupied.
    for _ in 0..30 {
        sequence
            .advance(
                Duration::from_millis(10),
                &mut keyboard,
                &mut machine,
                &mut runtime,
                KeyModifiers::NONE,
            )
            .unwrap();
        // Assert: No key or modifier ownership returns at any sampled boundary.
        assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
        assert_eq!(machine.read_byte(0x87aa).unwrap() & 3, 3);
    }
}

// What: Preserve exclusive ownership across manual input, panel replacement and cancellation.
// Why: A late host release must not release a newer sequenced key or allow a macro to displace it.
#[test]
fn neutral_ownership_rejects_busy_macros_and_ignores_stale_manual_releases() {
    // Arrange: Hold a translated manual key with inverted Shift on the keyboard PIA.
    let (mut machine, _) = running_keyboard();
    let mut keyboard = KeyboardState::default();
    let mut sequence = InputSequence::default();
    keyboard.press_manual(&mut machine, ManualKeyId(1), b'A', true, KeyModifiers::NONE);
    assert_eq!(machine.read_byte(0x87aa).unwrap() & 1, 0);

    // Act: Reject a macro, then let a panel key replace the manual owner before its release arrives.
    assert_eq!(
        sequence.queue_auto_load(&mut keyboard, &mut machine),
        Err("keyboard input is busy")
    );
    sequence.press(0x60, KeyModifiers::NONE, &mut keyboard, &mut machine);
    keyboard.release_manual(&mut machine);

    // Assert: The replacement key still owns unmodified input and also rejects macro displacement.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0xe0);
    assert_eq!(machine.read_byte(0x87aa).unwrap() & 3, 3);
    assert_eq!(
        sequence.queue_auto_load(&mut keyboard, &mut machine),
        Err("keyboard input is busy")
    );

    // Act: Cancel the panel input and submit a macro only after ownership is released.
    sequence.cancel(&mut keyboard, &mut machine);
    assert_eq!(machine.read_byte(0x87a8).unwrap(), 0x80);
    sequence
        .queue_auto_load(&mut keyboard, &mut machine)
        .unwrap();

    // Assert: The new macro starts with FIND's first key rather than resuming the cancelled panel key.
    assert_eq!(machine.read_byte(0x87a8).unwrap(), b'F' | 0x80);
}
