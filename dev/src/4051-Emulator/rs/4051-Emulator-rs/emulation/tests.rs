//! Exercises session timing, neutral snapshots and retained screen-copy state.

use super::super::test_support::observe_cursor_glyph;
use super::super::*;
use super::native_backend::{Backend, InlineBackend};
use std::path::PathBuf;
use tek4051_emulator::{
    emulation_input::{InputAction, InputFrame, KeyModifiers, ManualKeyId},
    host_runtime::HostRuntime,
    machine::cpu,
};

fn counting_machine() -> Machine {
    let mut machine = Machine::default();
    for (offset, byte) in [0x08, 0x20, 0xfd].into_iter().enumerate() {
        machine.write_byte(0x0100 + offset as u16, byte).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    machine
}

// What: Reuse a cursor mark observed while the first scripted command is still being typed.
// Why: The next program line must wait for a prompt emitted after the submitted line returns.
#[test]
fn program_load_requires_a_fresh_prompt_after_each_command() {
    // Arrange: Start a running session and queue a binary launch while its keyboard script is active.
    let mut state = EmulationState::new(counting_machine());
    state.apply(Command::Run(true)).unwrap();
    state
        .data
        .input_sequence
        .queue_binary_program_load(2, &mut state.data.keyboard, &mut state.data.machine)
        .unwrap();
    observe_cursor_glyph(&mut state);

    // Act: Let FIND finish while only the stale pre-command cursor mark is available.
    let mut now = state.data.last_update;
    for _ in 0..100 {
        now += Duration::from_millis(10);
        state.advance_realtime(now);
    }

    // Assert: Stale readiness cannot start BOLD; the keyboard is released awaiting a new prompt.
    let remaining_commands = 2 * "CALL \"BOLD\",5\rRUN\r".len();
    assert_eq!(state.pending_input_transitions(), remaining_commands);
    assert!(!state.data.basic_prompt_ready);

    // Act: Supply a new cursor mark after FIND has been fully sent.
    observe_cursor_glyph(&mut state);
    now += Duration::from_millis(10);
    state.advance_realtime(now);

    // Assert: The fresh readiness event starts BOLD's CALL command.
    assert!(state.pending_input_transitions() < remaining_commands);
}

#[test]
fn screen_file_effect_defers_writing_and_retains_the_selected_capture() {
    // What: Preparing a save emits captured data without writing or reporting completion.
    // Why: Native file execution must be separate from session state and use event-time pixels.
    // Arrange: Capture a dark frame, then flash the live display and choose a workspace path.
    let directory = tempfile::Builder::new()
        .prefix(".file-effect-tests-")
        .tempdir_in(".")
        .unwrap();
    let path = directory.path().join("capture.png");
    let mut state = EmulationState::new(counting_machine());
    let _ = state.apply(Command::RequestScreenCopy).unwrap();
    let id = state.snapshot().screen_copy_requests[0].0;
    state.data.machine.erase_display();

    // Act: Reject an unknown capture, then prepare the actual save without executing its effect.
    let error = state
        .apply(Command::SaveScreen {
            id: id + 1,
            path: path.clone(),
        })
        .err()
        .expect("unknown captures must be rejected");
    assert!(error.contains("unknown screen copy request"));
    assert_eq!(state.snapshot().screen_copy_requests[0].0, id);
    let effect = state
        .apply(Command::SaveScreen {
            id,
            path: path.clone(),
        })
        .unwrap()
        .expect("a valid save must produce a file effect");
    let prepared = state.snapshot();

    // Assert: Preparation consumed only the selected request, not the filesystem or notice queue.
    assert!(!path.exists());
    assert!(prepared.screen_copy_requests.is_empty());
    assert!(prepared.screen_copy_notices.is_empty());
    assert!(prepared.pixels.iter().all(|&pixel| pixel == 255));

    // Act: Execute the effect on the native owner and return its outcome to the session.
    let outcome = super::host_io::execute(effect, &mut state);
    state.complete_file_effect(outcome);
    let saved = image::open(&path).unwrap().to_rgba8();

    // Assert: Completion writes the earlier dark capture and publishes the existing success notice.
    assert!(saved.pixels().all(|pixel| pixel.0 == [0, 0, 0, 255]));
    let completed = state.snapshot();
    assert!(!completed.screen_copy_notices.last().unwrap().0);
    assert!(
        completed
            .screen_copy_notices
            .last()
            .unwrap()
            .1
            .contains("Screen copy saved:")
    );
    assert!(completed.runtime_error.is_none());
}

#[test]
fn file_commands_report_outcomes_and_save_the_requested_capture() {
    // What: imports, exports and screen saves publish outcomes through the same command path.
    // Why: moving host I/O off the GUI must preserve raw file bytes and captured screen content.
    // Arrange: keep all test outputs under the workspace and capture the initial dark screen.
    let directory = tempfile::Builder::new()
        .prefix(".emulation-tests-")
        .tempdir_in(".")
        .unwrap();
    let mut backend = InlineBackend::new(counting_machine());
    backend.command(Command::RequestScreenCopy).unwrap();
    let capture = backend
        .snapshot()
        .unwrap()
        .unwrap()
        .snapshot
        .screen_copy_requests[0]
        .0;
    let bytes = b"10 PRINT \"TEST\"\r";
    let export = directory.path().join("export.txt");
    let screen = directory.path().join("screen.png");

    // Act: import/export bytes, then change the display before saving the retained capture.
    backend
        .command(Command::ImportFiles(vec![FlashDriveFile {
            name: "7      ASCII   PROG Test 999".to_owned(),
            bytes: bytes.to_vec(),
        }]))
        .unwrap();
    backend
        .command(Command::ExportFile {
            number: 7,
            path: export.clone(),
        })
        .unwrap();
    backend.state.data.machine.erase_display();
    backend
        .command(Command::SaveScreen {
            id: capture,
            path: screen.clone(),
        })
        .unwrap();
    let snapshot = backend.snapshot().unwrap().unwrap().snapshot;
    let saved = image::ImageReader::open(&screen)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();

    // Assert: notices are successful and saving used the old dark frame, not the erase flash.
    assert_eq!(std::fs::read(export).unwrap(), bytes);
    assert!(!snapshot.import_notices[0].0);
    assert!(!snapshot.export_notice.as_ref().unwrap().0);
    assert!(snapshot.screen_copy_requests.is_empty());
    assert!(!snapshot.screen_copy_notices.last().unwrap().0);
    assert!(saved.pixels().all(|pixel| pixel.0 == [0, 0, 0, 255]));

    // Act: export a missing number to exercise an off-GUI failure result.
    backend
        .command(Command::ExportFile {
            number: 8,
            path: directory.path().join("missing"),
        })
        .unwrap();

    // Assert: file-operation failure is visible without stopping the paused emulator.
    let failed = backend.snapshot().unwrap().unwrap().snapshot;
    assert!(failed.export_notice.as_ref().unwrap().0);
    assert!(failed.runtime_error.is_none());
}

#[test]
fn file_io_completion_restores_modal_state_and_preserves_erase_wall_time() {
    // What: File-I/O time expires erase callbacks without CPU work and restores an outer suspension.
    // Why: Encapsulating worker state must preserve timing and acknowledged modal boundaries.
    // Arrange: Run the RAM loop with an erase, both with and without an outer file-operation modal.
    for outer_modal in [false, true] {
        let mut state = EmulationState::new(counting_machine());
        let _ = state.apply(Command::Run(true)).unwrap();
        if outer_modal {
            let _ = state.apply(Command::BeginFileOperation(9)).unwrap();
        }
        state.data.machine.erase_display();
        let started = state.data.last_update;
        let before = state.snapshot().registers;

        // Act: Use the production suspension/completion API across one second of file time.
        let previous_modal = state.begin_file_io();
        state.finish_file_io(started + Duration::from_secs(1), previous_modal);
        let finished = state.snapshot();

        // Assert: CPU state is frozen, erase completed, and the prior modal boundary remains.
        assert_eq!(finished.registers, before);
        assert!(finished.pixels.iter().all(|&pixel| pixel == 0));
        assert!(!state.data.machine.display_erase_pending());
        assert_eq!(finished.busy, outer_modal);
        assert!(finished.running);
        if outer_modal {
            assert_eq!(finished.file_operation, 9);
        }

        // Act: Advance one normal tick after completion using accepted session modifiers.
        state.advance_to(started + Duration::from_millis(1_010));
        let later = state.snapshot();

        // Assert: Only a session without the outer suspension resumes CPU execution.
        if outer_modal {
            assert_eq!(later.registers, before);
        } else {
            assert_ne!(later.registers.ix, before.ix);
        }
        assert!(later.runtime_error.is_none());
    }
}

#[test]
fn fatal_command_cleanup_releases_input_and_retains_the_error_while_stopped() {
    // What: Fatal command cleanup pauses execution, clears readiness/input and retains the error.
    // Why: The native owner must not manipulate runtime or keyboard fields to stop safely.
    // Arrange: Seed the shared detector with a complete semantic cursor glyph.
    use tek4051_emulator::emulation_input::{InputAction, ManualKeyId};
    let mut machine = counting_machine();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    let mut state = EmulationState::new(machine);
    let _ = state.apply(Command::Run(true)).unwrap();
    observe_cursor_glyph(&mut state);

    // Act: Hold a modified key through normal input while the detector is ready.
    let modifiers = KeyModifiers {
        shift: true,
        control: true,
    };
    let _ = state
        .apply(Command::Input(Box::new(InputFrame {
            actions: vec![InputAction::ManualPress {
                key: ManualKeyId(1),
                code: b'A',
                invert_shift: false,
                modifiers,
            }],
            modifiers,
            focused: true,
            reset_input: false,
        })))
        .unwrap();
    assert_eq!(state.data.machine.read_byte(0x87a8).unwrap(), 0xc1);
    assert_eq!(state.data.machine.read_byte(0x87aa).unwrap() & 3, 0);

    // Act: Feed the real unknown-capture error into the production failure operation.
    let error = state
        .apply(Command::SaveScreen {
            id: 0,
            path: PathBuf::from("unused.png"),
        })
        .err()
        .expect("an unknown capture must fail before producing an effect");
    state.fail(error.clone());
    let stopped = state.snapshot();
    state.advance_to(state.data.last_update + Duration::from_secs(1));
    state.stop();
    let later = state.snapshot();

    // Assert: Key/modifier lines and readiness clear; CPU stays frozen and error remains visible.
    assert_eq!(state.data.machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(state.data.machine.read_byte(0x87aa).unwrap() & 3, 3);
    assert!(!stopped.running);
    assert!(!stopped.basic_prompt_ready);
    assert_eq!(stopped.runtime_error.as_ref(), Some(&error));
    assert_eq!(later.registers, stopped.registers);
    assert_eq!(later.runtime_error.as_ref(), Some(&error));
    assert!(!later.running);
}

#[test]
fn runtime_execution_error_releases_sequenced_keyboard_input() {
    // What: A machine execution error releases active browser input and its modifiers.
    // Why: A stopped runtime cannot advance the sequence to deliver its scheduled keyup.
    // Arrange: Queue a released successor behind a shifted/control key before an unsupported opcode.
    let mut machine = counting_machine();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    machine.write_byte(0x0100, 0x02).unwrap();
    let mut state = EmulationState::new(machine);
    let _ = state.apply(Command::Run(true)).unwrap();
    observe_cursor_glyph(&mut state);
    let modifiers = KeyModifiers {
        shift: true,
        control: true,
    };
    let _ = state
        .apply(Command::Input(Box::new(InputFrame {
            actions: vec![
                InputAction::SequencedManualPress {
                    key: ManualKeyId(9),
                    code: b'A',
                    invert_shift: false,
                    modifiers,
                },
                InputAction::SequencedManualRelease(ManualKeyId(9)),
                InputAction::SequencedManualPress {
                    key: ManualKeyId(10),
                    code: b'B',
                    invert_shift: false,
                    modifiers: KeyModifiers::NONE,
                },
                InputAction::SequencedManualRelease(ManualKeyId(10)),
            ],
            modifiers,
            focused: true,
            reset_input: false,
        })))
        .unwrap();
    assert_eq!(state.data.machine.read_byte(0x87a8).unwrap(), 0xc1);
    assert_eq!(state.data.machine.read_byte(0x87aa).unwrap() & 3, 0);

    // Act: Advance through the unsupported opcode and let the runtime error stop execution.
    let now = state.data.last_update + Duration::from_millis(10);
    state.advance_to(now);
    let stopped = state.snapshot();

    // Assert: The error is visible and cancellation releases both the key and modifier lines.
    assert!(stopped.runtime_error.is_some());
    assert_eq!(state.data.machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(state.data.machine.read_byte(0x87aa).unwrap() & 3, 3);

    // Act: Resume only the paused input clock to expose any key left queued by the error path.
    let mut input_probe_runtime = HostRuntime::start(&mut state.data.machine);
    input_probe_runtime.pause();
    state
        .data
        .input_sequence
        .advance(
            Duration::from_millis(250),
            &mut state.data.keyboard,
            &mut state.data.machine,
            &mut input_probe_runtime,
            KeyModifiers::NONE,
        )
        .unwrap();

    // Assert: The queued successor stays absent after enough time for it to have appeared.
    assert_eq!(state.data.machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(state.data.machine.read_byte(0x87aa).unwrap() & 3, 3);
}

#[test]
fn coalescing_limits_cpu_work_but_preserves_erase_walltime() {
    // What: a long worker delay executes one tick while completing an overdue erase.
    // Why: overload must not create unbounded catch-up or prolong wall-time erase deadlines.
    // Arrange: start a counting loop with a fresh erase and one second of elapsed time.
    let mut state = EmulationState::new(counting_machine());
    let _ = state.apply(Command::Run(true)).unwrap();
    state.data.machine.erase_display();
    let now = state.data.last_update + Duration::from_secs(1);

    // Act: advance using the real-time coalescing policy.
    state.advance_realtime(now);

    // Assert: exactly one default instruction budget ran and the erase timeout elapsed.
    assert_eq!(state.data.machine.registers().ix, 1_132);
    assert!(!state.data.machine.display_erase_pending());
    assert_eq!(state.data.coalesced_time, Duration::from_millis(990));
}

#[test]
fn neutral_snapshots_reuse_pixels_and_retain_screen_captures_until_resolved() {
    // What: fresh metadata snapshots share unchanged pixels and retain bounded screen requests.
    // Why: polling must avoid framebuffer copies, while saving must use the captured frame.
    // Arrange: construct the session directly, without a native presentation adapter.
    let mut state = EmulationState::new(counting_machine());
    let initial = state.snapshot();

    // Act: change settings and capture once without advancing display state.
    let _ = state.apply(Command::CapsLock(true)).unwrap();
    let _ = state.apply(Command::RequestScreenCopy).unwrap();
    let captured = state.snapshot();
    state.data.machine.erase_display();
    let changed = state.snapshot();
    let (id, pixels) = &captured.screen_copy_requests[0];
    let _ = state.apply(Command::CancelScreenCopy(*id)).unwrap();
    let cancelled = state.snapshot();

    // Assert: metadata is fresh, the capture stays immutable, and cancellation releases it.
    assert!(captured.caps_lock);
    assert!(Arc::ptr_eq(&initial.pixels, &captured.pixels));
    assert_eq!(pixels.as_ref(), initial.pixels.as_ref());
    assert!(initial.pixels.iter().all(|&pixel| pixel == 0));
    assert!(changed.pixels.iter().all(|&pixel| pixel == 255));
    assert!(!Arc::ptr_eq(&captured.pixels, &changed.pixels));
    assert!(Arc::ptr_eq(pixels, &changed.screen_copy_requests[0].1));
    assert_ne!(changed.revision, captured.revision);
    assert!(cancelled.screen_copy_requests.is_empty());
    assert_eq!(
        cancelled.screen_copy_notices.last().unwrap().1,
        "Screen copy cancelled"
    );
}

#[test]
fn screen_capture_overflow_is_bounded_and_reported() {
    // What: unresolved captures cannot grow without bound.
    // Why: repeated device requests while dialogs are open must not exhaust memory.
    // Arrange: retain all captures in the deterministic backend.
    let mut backend = InlineBackend::new(counting_machine());

    // Act: request one capture more than the pending capture limit.
    for _ in 0..=MAX_SCREEN_COPIES {
        backend.command(Command::RequestScreenCopy).unwrap();
    }
    let snapshot = backend.snapshot().unwrap().unwrap().snapshot;

    // Assert: existing requests remain available and the rejected request is visible.
    assert_eq!(snapshot.screen_copy_requests.len(), MAX_SCREEN_COPIES);
    assert!(
        snapshot
            .screen_copy_notices
            .iter()
            .any(|(error, message)| *error && message.contains("queue full"))
    );
}

#[test]
fn modal_focus_loss_releases_accepted_keys_and_modifiers_before_resuming() {
    // What: Clear an accepted modified key through the neutral modal focus-loss path.
    // Why: Modal input rejection must not prevent release cleanup or leave modifier lines latched.
    // Arrange: Hold Shift/Control/A using the production command handler and exposed PIA ports.
    use tek4051_emulator::emulation_input::{InputAction, ManualKeyId};
    let mut backend = InlineBackend::new(counting_machine());
    backend.state.data.machine.write_byte(0x87a9, 0x04).unwrap();
    backend.state.data.machine.write_byte(0x87ab, 0x04).unwrap();
    backend.command(Command::Run(true)).unwrap();
    observe_cursor_glyph(&mut backend.state);
    let held = KeyModifiers {
        shift: true,
        control: true,
    };
    backend
        .command(Command::Input(Box::new(InputFrame {
            actions: vec![InputAction::ManualPress {
                key: ManualKeyId(1),
                code: b'A',
                invert_shift: false,
                modifiers: held,
            }],
            modifiers: held,
            focused: true,
            reset_input: false,
        })))
        .unwrap();
    assert_eq!(backend.state.data.machine.read_byte(0x87a8).unwrap(), 0xc1);
    assert_eq!(backend.state.data.machine.read_byte(0x87aa).unwrap() & 3, 0);

    // Act: Suspend normal input, then deliver focus loss without ever leaving modal mode.
    backend.command(Command::Modal(true)).unwrap();
    backend
        .command(Command::Input(Box::new(InputFrame::unfocused())))
        .unwrap();

    // Assert: Both key and modifier lines clear while the modal suspension is still acknowledged.
    assert_eq!(backend.state.data.machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(backend.state.data.machine.read_byte(0x87aa).unwrap() & 3, 3);
    assert!(backend.state.data.keyboard.is_idle());
    assert!(backend.snapshot().unwrap().unwrap().snapshot.busy);
}

#[test]
fn canonical_export_names_match_core_without_copying_payloads() {
    // What: snapshot filenames match canonical core export names, including Unicode errors.
    // Why: native Save As must retain the numbered-file import/export naming convention.
    // Arrange: import named ASCII and BINARY files with meaningful non-ASCII metadata.
    let mut backend = InlineBackend::new(counting_machine());
    backend
        .command(Command::ImportFiles(vec![
            FlashDriveFile {
                name: "7      ASCII   PROG Grüß😀 999".to_owned(),
                bytes: b"TEST\r".to_vec(),
            },
            FlashDriveFile {
                name: "8      BINARY  DATA Values 999".to_owned(),
                bytes: vec![0, 255],
            },
            FlashDriveFile {
                name: "9      ASCII   PROG abcdefghijklmno😀 999".to_owned(),
                bytes: vec![],
            },
        ]))
        .unwrap();

    // Act: publish metadata and obtain the core's canonical reference filenames.
    let snapshot = backend.snapshot().unwrap().unwrap().snapshot;
    let expected: Vec<_> = [7, 8]
        .into_iter()
        .map(|number| {
            (
                number,
                backend
                    .state
                    .data
                    .machine
                    .export_flash_file(number)
                    .unwrap()
                    .name,
            )
        })
        .collect();
    // Assert: metadata-only names agree byte-for-byte, and invalid UTF-16 remains an error.
    for (number, name) in expected {
        assert_eq!(snapshot.export_names[&number].as_ref().unwrap(), &name);
    }
    assert!(snapshot.export_names[&9].is_err());
}

#[test]
fn overflow_notice_survives_capture_completion_and_inline_notice_replacement() {
    // What: device overflow remains visible after the GUI handles retained captures.
    // Why: draining device requests into snapshots must not hide previously dropped requests.
    // Arrange: queue eight device copies and overflow two more at the machine boundary.
    let mut backend = InlineBackend::new(counting_machine());
    backend.state.data.machine.write_byte(0x8795, 0x04).unwrap();
    for _ in 0..10 {
        backend.state.data.machine.write_byte(0x8794, 0x20).unwrap();
        backend.state.data.machine.write_byte(0x8794, 0).unwrap();
    }

    // Act: publish, finish all captures, then simulate the inline adapter replacing notices.
    let snapshot = backend.snapshot().unwrap().unwrap().snapshot;
    for (id, _) in &snapshot.screen_copy_requests {
        backend.command(Command::CancelScreenCopy(*id)).unwrap();
    }
    backend.state.data.screen_copy_notices.clear();
    let resolved = backend.snapshot().unwrap().unwrap().snapshot;

    // Assert: all buffered captures were exposed and both dropped requests are still reported.
    assert_eq!(snapshot.screen_copy_requests.len(), 8);
    assert!(
        resolved
            .screen_copy_notices
            .iter()
            .any(|(error, message)| *error && message.contains("At least 2"))
    );
    assert!(resolved.runtime_error.is_none());
}
