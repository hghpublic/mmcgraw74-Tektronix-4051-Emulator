//! External consumers exercise session execution without native adapters or private data.

use std::{
    io::Cursor,
    path::PathBuf,
    time::{Duration, Instant},
};

use tek4051_emulator::{
    emulation::{
        Command, CpuReferenceRenderer, EmulationState, FileEffect, FileOutcome,
        input::{InputAction, InputFrame, KeyModifiers, ManualKeyId},
        keyboard::map_text_character_with_host_shift,
    },
    machine::{Machine, cpu, gpib::FlashDriveFile},
};

fn counting_machine() -> Machine {
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for (offset, byte) in [0x08, 0x20, 0xfd].into_iter().enumerate() {
        machine.write_byte(0x0100 + offset as u16, byte).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    machine
}

#[test]
fn prepared_machine_and_nondecreasing_clock_preserve_pause_resume_boundaries() {
    // What: Prepared registers survive construction and accounted pause time is never executed.
    // Why: External hosts must initialize the machine and advance time before Run/Pause changes.
    // Arrange: Reset hardware, install the RAM loop, and establish the first paused host timestamp.
    let machine = counting_machine();
    let prepared = machine.registers();
    let mut session = EmulationState::new(machine);
    assert_eq!(session.snapshot().registers, prepared);
    let started = Instant::now();
    session.advance_to(started);

    // Act: Run one tick, pause, and account for a long interval before resuming at the same time.
    session.apply(Command::Run(true)).unwrap();
    session.advance_to(started + Duration::from_millis(10));
    let running = session.snapshot();
    session.apply(Command::Run(false)).unwrap();
    let resumed_at = started + Duration::from_secs(5);
    session.advance_to(resumed_at);
    let paused = session.snapshot();
    session.apply(Command::Run(true)).unwrap();
    session.advance_to(resumed_at);
    let resumed = session.snapshot();
    session.advance_to(resumed_at + Duration::from_millis(10));
    let next_tick = session.snapshot();

    // Assert: Constructor retains preparation, repeated time advances nothing, and only a new tick runs.
    assert!(running.registers.ix > prepared.ix);
    assert_eq!(paused.registers, running.registers);
    assert_eq!(resumed.registers, paused.registers);
    assert!(resumed.running);
    assert!(next_tick.registers.ix > resumed.registers.ix);
    assert!(next_tick.runtime_error.is_none());
}

// What: A host key reaches the machine keyboard path before the ROM draws its cursor.
// Why: BASIC readiness is informative, not permission for the host to drop physical-keyboard input.
#[test]
fn host_key_reaches_machine_before_cursor_readiness() {
    // Arrange: Start a freshly reset ROM machine with no detected BASIC cursor.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut session = EmulationState::new(machine);
    let mut now = Instant::now();
    session.advance_to(now);
    session.apply(Command::Run(true)).unwrap();
    let press = || {
        Box::new(InputFrame {
            actions: vec![InputAction::SequencedManualPress {
                key: ManualKeyId(1),
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            }],
            modifiers: KeyModifiers::NONE,
            focused: true,
            reset_input: false,
        })
    };

    // Act: Send an early press and release it before the startup CPU loop runs.
    session.apply(Command::Input(press())).unwrap();
    let early = session.snapshot();
    assert!(!early.basic_prompt_ready);
    assert!(session.has_held_key());
    assert!(early.input_notice.is_none());
    session
        .apply(Command::Input(Box::new(InputFrame::unfocused())))
        .unwrap();
    assert!(!session.has_held_key());

    // Act: Advance startup until the core detects a complete cursor glyph.
    for _ in 0..3_000 {
        if early.basic_prompt_ready || session.snapshot().basic_prompt_ready {
            break;
        }
        now += Duration::from_millis(10);
        session.advance_to(now);
    }
    let ready = session.snapshot();

    // Assert: Input was delivered before readiness, and the ROM independently signals its prompt.
    assert!(
        ready.basic_prompt_ready,
        "the ROM must redraw its cursor mark"
    );

    // Act: Queue browser-style taps at the current 50/30 ms cadence while editing the BASIC line.
    // The cursor-based readiness badge may be temporarily absent during dense editing; it does not gate these keys.
    for (index, code) in b"PRINT\x101\r".iter().copied().enumerate() {
        let key = ManualKeyId(index as u64 + 2);
        session
            .apply(Command::Input(Box::new(InputFrame {
                actions: vec![
                    InputAction::SequencedManualPress {
                        key,
                        code,
                        invert_shift: false,
                        modifiers: KeyModifiers::NONE,
                    },
                    InputAction::SequencedManualRelease(key),
                ],
                modifiers: KeyModifiers::NONE,
                focused: true,
                reset_input: false,
            })))
            .unwrap();
        if code == b'\r' {
            assert!(!session.snapshot().basic_prompt_ready);
        }
        now += Duration::from_millis(80);
        session.advance_to(now);
    }

    let output_line = 82 * CpuReferenceRenderer::WIDTH..96 * CpuReferenceRenderer::WIDTH;
    for _ in 0..3_000 {
        let snapshot = session.snapshot();
        let output_pixels = snapshot.pixels[output_line.clone()]
            .iter()
            .filter(|&&pixel| pixel >= 240)
            .count();
        if snapshot.basic_prompt_ready && output_pixels == 40 {
            break;
        }
        now += Duration::from_millis(10);
        session.advance_to(now);
    }

    // Assert: Return invalidated the old signal, the ROM redrew its cursor, and PRINT rendered "1".
    let snapshot = session.snapshot();
    let pixels = &snapshot.pixels;
    assert!(snapshot.basic_prompt_ready);
    assert_eq!(
        pixels[output_line]
            .iter()
            .filter(|&&pixel| pixel >= 240)
            .count(),
        40,
        "final readiness={}, notice={:?}, pc={:#06x}",
        snapshot.basic_prompt_ready,
        snapshot.input_notice,
        snapshot.registers.pc
    );
    assert!(!session.has_held_key());
}

fn advance_until_basic_ready(session: &mut EmulationState, now: &mut Instant) -> bool {
    for _ in 0..3_000 {
        if session.snapshot().basic_prompt_ready {
            return true;
        }
        *now += Duration::from_millis(10);
        session.advance_to(*now);
    }
    session.snapshot().basic_prompt_ready
}

fn type_basic_text(
    session: &mut EmulationState,
    now: &mut Instant,
    text: &str,
    first_key_id: u64,
) -> bool {
    let mut return_cleared_readiness = false;
    for (index, character) in text.chars().chain(['\r']).enumerate() {
        let (code, invert_shift) = if character == '\r' {
            (b'\r', false)
        } else {
            let mapping = map_text_character_with_host_shift(character, false)
                .expect("test command text should be printable");
            (mapping.code, mapping.invert_shift)
        };
        let key = ManualKeyId(first_key_id + index as u64);
        session
            .apply(Command::Input(Box::new(InputFrame {
                actions: vec![
                    InputAction::SequencedManualPress {
                        key,
                        code,
                        invert_shift,
                        modifiers: KeyModifiers::NONE,
                    },
                    InputAction::SequencedManualRelease(key),
                ],
                modifiers: KeyModifiers::NONE,
                focused: true,
                reset_input: false,
            })))
            .unwrap();
        if character == '\r' {
            return_cleared_readiness = !session.snapshot().basic_prompt_ready;
        }
        *now += Duration::from_millis(200);
        session.advance_to(*now);
    }
    return_cleared_readiness
}

// What: Reacquire BASIC readiness at a program INPUT prompt and allow sequenced host input.
// Why: The blinking question mark is transient even though it is not the 5-by-8 system cursor.
#[test]
fn basic_input_question_mark_rearms_readiness_and_allows_host_input() {
    // Arrange: Start the reset ROM and wait for its first BASIC command prompt.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut session = EmulationState::new(machine);
    let mut now = Instant::now();
    session.advance_to(now);
    session.apply(Command::Run(true)).unwrap();
    assert!(advance_until_basic_ready(&mut session, &mut now));

    // Act: Enter both BASIC lines, run them, and wait for the blinking input prompt.
    assert!(type_basic_text(&mut session, &mut now, "10 INPUT A", 1));
    assert!(advance_until_basic_ready(&mut session, &mut now));
    assert!(type_basic_text(&mut session, &mut now, "20 PRINT A", 20));
    assert!(advance_until_basic_ready(&mut session, &mut now));
    assert!(type_basic_text(&mut session, &mut now, "RUN", 40));
    assert!(advance_until_basic_ready(&mut session, &mut now));

    // Assert: The transient program prompt independently re-arms the shared BASIC readiness signal.
    assert!(session.snapshot().basic_prompt_ready);

    // Act: Send 5 and Return through the sequenced browser-style input path.
    assert!(type_basic_text(&mut session, &mut now, "5", 100));

    // Assert: The session accepts the key sequence without an ignored-input notice or held key.
    assert!(session.snapshot().input_notice.is_none());
    assert!(!session.has_held_key());
}

fn run_basic_command(
    command: &str,
    host_shift_for: impl Fn(char) -> bool,
    tap_interval: Duration,
) -> Vec<u8> {
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut session = EmulationState::new(machine);
    let mut now = Instant::now();
    session.advance_to(now);
    session.apply(Command::Run(true)).unwrap();

    for _ in 0..3_000 {
        if session.snapshot().basic_prompt_ready {
            break;
        }
        now += Duration::from_millis(10);
        session.advance_to(now);
    }
    assert!(session.snapshot().basic_prompt_ready);

    for (index, character) in command.chars().chain(['\r']).enumerate() {
        let host_shift = host_shift_for(character);
        let (code, invert_shift) = if character == '\r' {
            (b'\r', false)
        } else {
            let mapping = map_text_character_with_host_shift(character, host_shift)
                .expect("command text should be printable");
            (mapping.code, mapping.invert_shift)
        };
        let modifiers = KeyModifiers {
            shift: host_shift,
            control: false,
        };
        let key = ManualKeyId(index as u64 + 1);
        session
            .apply(Command::Input(Box::new(InputFrame {
                actions: vec![
                    InputAction::SequencedManualPress {
                        key,
                        code,
                        invert_shift,
                        modifiers,
                    },
                    InputAction::SequencedManualRelease(key),
                ],
                modifiers,
                focused: true,
                reset_input: false,
            })))
            .unwrap();
        now += tap_interval;
        session.advance_to(now);
    }

    let output_line = 82 * CpuReferenceRenderer::WIDTH..96 * CpuReferenceRenderer::WIDTH;
    for _ in 0..3_000 {
        let snapshot = session.snapshot();
        let output_pixels = snapshot.pixels[output_line.clone()]
            .iter()
            .any(|&pixel| pixel >= 240);
        if snapshot.basic_prompt_ready && !session.has_held_key() && output_pixels {
            break;
        }
        now += Duration::from_millis(10);
        session.advance_to(now);
    }
    let snapshot = session.snapshot();
    assert!(snapshot.basic_prompt_ready);
    assert!(!session.has_held_key());
    snapshot.pixels[output_line].to_vec()
}

// What: Compare a German-layout asterisk multiplication with the same literal result.
// Why: Paired text must preserve the typed symbol rather than leave the host's physical key interpretation.
#[test]
fn paired_asterisk_enters_as_multiplication_operator() {
    // Arrange: Run equivalent BASIC expressions through the real ROM and shared character mapping.
    let german_shift = |character| character == '*';
    let tap_interval = Duration::from_millis(100);
    let literal_six = run_basic_command("print 6", german_shift, tap_interval);
    let literal_pixels = literal_six.iter().filter(|&&pixel| pixel >= 240).count();

    // Act: Enter '*' with its host Shift modifier, as on a German keyboard's shifted plus key.
    let calculated_six = run_basic_command("print 2*3", german_shift, tap_interval);

    // Assert: The output line matches a literal 6, not an operator substitution or syntax error.
    assert_eq!(
        literal_pixels, 64,
        "PRINT 6 should visibly render the expected digit"
    );
    assert_eq!(calculated_six, literal_six);
}

// What: Enter addition with the unshifted German plus key through the real ROM.
// Why: The host's event-time Shift state must not turn the mapped plus into another character.
#[test]
fn unshifted_german_plus_enters_as_the_addition_operator() {
    // Arrange: Capture the real-ROM output for the equivalent literal result.
    let no_shift = |_| false;
    let tap_interval = Duration::from_millis(100);
    let expected = run_basic_command("print 5", no_shift, tap_interval);

    // Act: Enter an equivalent expression with German plus unshifted.
    let actual = run_basic_command("print 2+3", no_shift, tap_interval);

    // Assert: BASIC interprets the mapped symbol as addition and prints the expected digit.
    assert_eq!(actual, expected);
}

#[test]
fn headless_execution_retains_capture_pixels_and_reports_host_failure() {
    // What: A library consumer runs, captures and pauses a real session without native widgets.
    // Why: The exported owner must preserve CPU/erase timing and deferred event-time capture data.
    // Arrange: Begin with an erase flash and retain it before starting the masked RAM loop.
    let mut machine = counting_machine();
    machine.erase_display();
    let mut session = EmulationState::new(machine);
    session.apply(Command::RequestScreenCopy).unwrap();
    let initial = session.snapshot();
    let id = initial.screen_copy_requests[0].0;

    // Act: Start execution and coalesce a long wall-time interval through the public API.
    let started = Instant::now();
    session.advance_to(started);
    session.apply(Command::Run(true)).unwrap();
    let now = started + Duration::from_secs(1);
    session.advance_realtime(now);
    let advanced = session.snapshot();

    // Assert: CPU progressed, the erase expired, and publications have the complete grayscale shape.
    assert!(!initial.running);
    assert!(initial.pixels.iter().all(|&pixel| pixel == 255));
    assert!(advanced.running);
    assert!(advanced.registers.ix > initial.registers.ix);
    assert_eq!(
        advanced.pixels.len(),
        CpuReferenceRenderer::WIDTH * CpuReferenceRenderer::HEIGHT
    );
    assert!(advanced.pixels.iter().all(|&pixel| pixel == 0));
    assert!(advanced.coalesced_time >= Duration::from_millis(990));

    // Act: Reject a stale ID, prepare the retained capture, and complete a failed host write.
    let path = PathBuf::from("unused-capture.png");
    assert!(
        session
            .apply(Command::SaveScreen {
                id: id + 1,
                path: path.clone()
            })
            .is_err()
    );
    assert_eq!(session.snapshot().screen_copy_requests[0].0, id);
    let effect = session
        .apply(Command::SaveScreen { id, path })
        .unwrap()
        .unwrap();
    let FileEffect::SaveScreen { pixels, .. } = effect else {
        panic!("a screen save must return the selected capture");
    };
    assert!(pixels.iter().all(|&pixel| pixel == 255));
    session.complete_file_effect(FileOutcome::Screen((
        true,
        "destination unavailable".to_owned(),
    )));
    session.advance_to(now);
    session.apply(Command::Run(false)).unwrap();
    let paused = session.snapshot();
    session.advance_to(now + Duration::from_millis(100));
    let later = session.snapshot();

    // Assert: No capture is pending, the host error is visible but nonfatal, and Pause freezes CPU.
    assert!(paused.screen_copy_requests.is_empty());
    assert_eq!(
        paused.screen_copy_notices.last(),
        Some(&(true, "destination unavailable".to_owned()))
    );
    assert!(paused.runtime_error.is_none());
    assert!(!later.running);
    assert_eq!(later.registers, paused.registers);
}

#[test]
fn public_archive_streams_preserve_failed_import_and_replace_on_success() {
    // What: Session archive streams validate before replacing storage and return reusable output.
    // Why: External hosts must not need mutable machine access to preserve transactional storage.
    // Arrange: Store an old file and prepare a replacement archive through a separate machine.
    let mut session = EmulationState::new(Machine::default());
    session
        .apply(Command::ImportFiles(vec![FlashDriveFile {
            name: "7      ASCII   PROG Old 999".to_owned(),
            bytes: b"10 PRINT \"OLD\"\r".to_vec(),
        }]))
        .unwrap();
    let replacement_bytes = b"20 PRINT \"NEW\"\r";
    let mut replacement = Machine::default();
    replacement
        .import_flash_files(&[FlashDriveFile {
            name: "9      ASCII   PROG New 999".to_owned(),
            bytes: replacement_bytes.to_vec(),
        }])
        .into_iter()
        .next()
        .unwrap()
        .unwrap();
    let mut archive = replacement
        .write_flash_storage_archive(Cursor::new(Vec::new()))
        .unwrap();
    archive.set_position(0);
    let was_modal = session.begin_file_io();

    // Act: Reject invalid archive bytes while the same owner is suspended.
    assert!(
        session
            .import_archive(Cursor::new(b"not a ZIP".to_vec()))
            .is_err()
    );
    let failed = session.snapshot();

    // Assert: The old store remains visible and import failure has not become a runtime failure.
    assert!(failed.busy);
    assert_eq!(
        failed
            .directory
            .iter()
            .map(|file| file.number)
            .collect::<Vec<_>>(),
        [7]
    );
    assert!(failed.runtime_error.is_none());

    // Act: Prepare an export before replacement to inspect the retained payload through public effects.
    let effect = session
        .apply(Command::ExportFile {
            number: 7,
            path: PathBuf::from("unused-old.txt"),
        })
        .unwrap()
        .unwrap();
    let FileEffect::ExportFile { file, .. } = effect else {
        panic!("a numbered export must return its stored payload");
    };
    // Assert: Failed validation retained exact content, not only the directory entry.
    assert_eq!(file.bytes, b"10 PRINT \"OLD\"\r");
    session.complete_file_effect(FileOutcome::Export((
        true,
        "test host did not write a destination".to_owned(),
    )));

    // Act: Commit the valid replacement, stream it out, and finish the matching I/O suspension.
    assert_eq!(session.import_archive(archive).unwrap(), 1);
    let mut output = session.write_archive(Cursor::new(Vec::new())).unwrap();
    output.set_position(0);
    let mut restored = Machine::default();
    restored.import_flash_storage_archive(output).unwrap();
    session.finish_file_io(Instant::now(), was_modal);
    let finished = session.snapshot();

    // Assert: Replacement removed the old file, output retains exact bytes, and modal state restores.
    assert_eq!(
        finished
            .directory
            .iter()
            .map(|file| file.number)
            .collect::<Vec<_>>(),
        [9]
    );
    assert_eq!(
        restored.export_flash_file(9).unwrap().bytes,
        replacement_bytes
    );
    assert!(!finished.busy);
    assert!(!finished.running);
}
