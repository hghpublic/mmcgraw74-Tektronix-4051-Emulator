use std::{
    fs::File,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::emulation::session::test_support::observe_cursor_glyph;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};
use tek4051_emulator::{
    emulation::CpuReferenceRenderer,
    host_runtime::{DISPLAY_ERASE_DELAY, HostRuntime},
    machine::Machine,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::gui::InlineApp as Tek4051App;
use super::host_import::{
    FlashArchiveBudget, MAX_FLASH_ARCHIVE_ENTRIES, MAX_FLASH_ARCHIVE_ENTRY_BYTES,
    MAX_FLASH_ARCHIVE_FILE_BYTES, MAX_FLASH_ARCHIVE_TOTAL_BYTES, read_flash_archive,
    read_flash_archive_from, read_flash_directory, read_flash_file, read_flash_paths,
    read_flash_paths_with_budget,
};
use super::host_io::{save_screen, write_flash_file, write_flash_storage_archive_to_path};
use super::{FlashExportRequest, FlashImportDialogs, FlashImportSelection, speed_control};
use tek4051_emulator::emulation_input::AutoLoadState;

// What: Toggle the real directory control, inspect metadata, and observe subsequent stored-file changes.
// Why: The optional view must stay read-only and current without changing framebuffer or Auto Load ownership.
#[test]
fn flash_directory_panel_shows_live_metadata_without_mutating_machine_state() {
    // Arrange: Import different stored kinds while paused and settle the native controls.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[
            tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "20     BINARY  DATA Payload 999".to_owned(),
                bytes: vec![0, 0xff, 4, 5]
            },
            tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "1      ASCII   TEXT Note 999".to_owned(),
                bytes: b"first\r".to_vec()
            },
        ]),
        [Ok(()), Ok(())]
    );
    let metadata = machine.flash_drive_directory();
    let registers = machine.registers();
    let mut app = Tek4051App::new(&creation, machine);
    let pixels = app.display().pixels().to_vec();
    let now = app.last_update;
    app_frame(&mut app, &context, now, vec![]);
    let hidden = app_frame(&mut app, &context, now, vec![]);
    assert!(
        !hidden
            .iter()
            .any(|(text, _)| text == "Flash Drive directory")
    );

    // Act: Open the directory with its painted checkbox.
    click_app_control(&mut app, &context, now, "Show directory");
    app_frame(&mut app, &context, now, vec![]);
    let visible = app_frame(&mut app, &context, now, vec![]);

    // Assert: Canonical labels, descriptions and byte counts are painted without starting execution.
    for expected in [
        "Flash Drive directory",
        "2 stored file(s)",
        "Number",
        "Type",
        "Usage",
        "Bytes",
        "Description",
        "ASCII",
        "TEXT",
        "Note",
        "BINARY",
        "DATA",
        "Payload",
        "6",
        "4",
    ] {
        assert!(
            visible.iter().any(|(text, _)| text == expected),
            "missing {expected}: {visible:?}"
        );
    }
    assert!(app.show_flash_directory);
    assert!(!app.running);
    assert_eq!(app.machine.flash_drive_directory(), metadata);
    assert_eq!(app.machine.registers(), registers);
    assert_eq!(app.display().pixels(), pixels);
    assert_eq!(app.export_number, 1);

    // Act: Replace stored metadata and add another file without reopening the view.
    assert_eq!(
        app.machine
            .import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "20     BINARY  DATA Changed 999".to_owned(),
                bytes: vec![1; 5],
            }]),
        [Ok(())]
    );
    app.machine.store_tape_program(3, &[]).unwrap();
    let refreshed = app_frame(&mut app, &context, now, vec![]);

    // Assert: The next frame displays changed descriptions/sizes and the new count.
    for expected in ["Changed", "5", "3 stored file(s)"] {
        assert!(
            refreshed.iter().any(|(text, _)| text == expected),
            "missing {expected}"
        );
    }
    assert!(!refreshed.iter().any(|(text, _)| text == "Payload"));
    assert_eq!(app.display().pixels(), pixels);
    click_app_control(&mut app, &context, now, "Show directory");
    let hidden_again = app_frame(&mut app, &context, now, vec![]);
    assert!(!app.show_flash_directory);
    assert!(
        !hidden_again
            .iter()
            .any(|(text, _)| text == "Flash Drive directory")
    );

    // Act/Assert: Reopening is also allowed during Auto Load and must not cancel its queued keys.
    click_app_control(&mut app, &context, now, "Start");
    let ready = advance_until_prompt(&mut app, &context, now);
    click_app_control(&mut app, &context, ready, "Auto Load");
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
    click_app_control(&mut app, &context, ready, "Show directory");
    assert!(app.show_flash_directory);
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
}

// What: Show empty storage, then malformed Unicode alongside valid marker and unsafe-export descriptions.
// Why: Read-only inspection must not hide stored entries or silently replace invalid UTF-16.
#[test]
fn flash_directory_panel_reports_invalid_description_while_stopped() {
    // Arrange: Prepare an empty stopped machine and files with a split surrogate, marker and path-like description.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let files = [
        "1      ASCII   PROG abcdefghijklmno\u{1f600} 999",
        "2      NEW          marker 999",
        "3      ASCII   DATA ../bad 999",
    ]
    .map(|name| tek4051_emulator::machine::gpib::FlashDriveFile {
        name: name.to_owned(),
        bytes: vec![9],
    });
    let mut app = Tek4051App::new(&creation, machine);
    app.runtime_error = Some("test stopped runtime".to_owned());
    let now = app.last_update;
    app_frame(&mut app, &context, now, vec![]);

    // Act: Open the actual control even though execution is stopped.
    click_app_control(&mut app, &context, now, "Show directory");
    let empty = app_frame(&mut app, &context, now, vec![]);
    // Assert: Empty storage is reported explicitly, not as missing UI or an error.
    assert!(empty.iter().any(|(text, _)| text == "No stored files"));

    // Act: Add mixed metadata while leaving the directory open.
    assert_eq!(
        app.machine.import_flash_files(&files),
        [Ok(()), Ok(()), Ok(())]
    );
    app_frame(&mut app, &context, now, vec![]);
    let visible = app_frame(&mut app, &context, now, vec![]);

    // Assert: All entries remain visible, with explicit Unicode error rather than replacement text.
    assert!(
        visible
            .iter()
            .any(|(text, _)| text.starts_with("Invalid UTF-16 description:"))
    );
    for expected in ["3 stored file(s)", "NEW", "-", "marker", "../bad"] {
        assert!(
            visible.iter().any(|(text, _)| text == expected),
            "missing {expected}: {visible:?}"
        );
    }
    assert!(!visible.iter().any(|(text, _)| text.contains('\u{fffd}')));
    assert!(!app.running);
    assert_eq!(app.runtime_error.as_deref(), Some("test stopped runtime"));
}

// What: Replace storage with an empty archive while a directory row owns focus.
// Why: Removed entries must not leave stale keyboard ownership in the native directory.
#[test]
fn flash_directory_empty_storage_clears_list_focus() {
    // Arrange: Focus the only stored row, then prepare a valid empty replacement archive.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "1      ASCII   PROG [Focus] 12.txt".to_owned(),
            bytes: b"10 PRINT 1\r\r".to_vec(),
        }]),
        [Ok(())]
    );
    let empty_archive = ZipWriter::new(Cursor::new(Vec::new()))
        .finish()
        .unwrap()
        .into_inner();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    app_frame(&mut app, &context, now, vec![]);
    click_app_control(&mut app, &context, now, "Show directory");
    let entries = app_frame(&mut app, &context, now, vec![]);
    click_app_position(
        &mut app,
        &context,
        now,
        directory_number_position(&entries, 1),
    );
    assert!(app.directory_list_focused);

    // Act: Import the empty archive and render the now-empty directory.
    app.machine
        .import_flash_storage_archive(Cursor::new(empty_archive))
        .unwrap();
    let empty_entries = app_frame(&mut app, &context, now, vec![]);

    // Assert: The view reports no files and relinquishes the removed row's focus state.
    assert!(
        empty_entries
            .iter()
            .any(|(text, _)| text == "No stored files")
    );
    assert!(!app.directory_list_focused);
    assert_eq!(app.selected_directory_file, None);
}

// What: Exercise detected prompt readiness, arrow selection and an ASCII PROG launch.
// Why: Directory actions must use machine-detected readiness without sending navigation keys to BASIC.
#[test]
fn flash_directory_keyboard_selection_launches_selected_program_through_rom() {
    // Arrange: Start BASIC and import entries that bracket the runnable ASCII program in file order.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[
            tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "1      ASCII   DATA Input 1".to_owned(),
                bytes: b"unused\r".to_vec(),
            },
            tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "2      BINARY  PROG [Unsupported] 12.bin".to_owned(),
                bytes: b"10 PRINT 2\r\r".to_vec(),
            },
            tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "3      ASCII   PROG [Directory] 12.txt".to_owned(),
                bytes: b"10 PRINT 1\r\r".to_vec(),
            },
        ]),
        [Ok(()), Ok(()), Ok(())]
    );
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    app_frame(&mut app, &context, start, vec![]);
    click_app_control(&mut app, &context, start, "Start");
    let mut ready = start;
    for _ in 0..3_000 {
        if app.basic_prompt_ready {
            break;
        }
        ready += Duration::from_millis(10);
        app_frame(&mut app, &context, ready, vec![]);
    }
    assert!(app.basic_prompt_ready, "the ROM must draw its BASIC cursor");
    click_app_control(&mut app, &context, ready, "Show directory");
    let entries = app_frame(&mut app, &context, ready, vec![]);
    click_app_position(
        &mut app,
        &context,
        ready,
        directory_number_position(&entries, 1),
    );
    assert_eq!(app.selected_directory_file, Some(1));

    // Act: Reject a DATA file, then navigate across a BINARY PROG to the ASCII PROG.
    send_app_key(&mut app, &context, ready, egui::Key::Enter);
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Idle);
    assert_eq!(
        app.program_launch_notice
            .as_ref()
            .map(|(_, notice)| notice.as_str()),
        Some("Flash Drive file 1 is not an ASCII or BINARY PROG")
    );
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowUp);
    assert_eq!(app.selected_directory_file, Some(1));
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowDown);
    assert_eq!(app.selected_directory_file, Some(2));

    // Act: Move through the list in both directions and attempt to cross each end.
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowUp);
    assert_eq!(app.selected_directory_file, Some(1));
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowDown);
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowDown);
    assert_eq!(app.selected_directory_file, Some(3));
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowDown);
    assert_eq!(app.selected_directory_file, Some(3));
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowUp);
    assert_eq!(app.selected_directory_file, Some(2));
    send_directory_arrow(&mut app, &context, ready, egui::Key::ArrowDown);
    assert_eq!(app.selected_directory_file, Some(3));
    // Assert: Navigation restores row focus and ends on the supported ASCII PROG.
    assert_eq!(app.selected_directory_file, Some(3));
    let navigated_entries = app_frame(&mut app, &context, ready, vec![]);
    let selected_position = directory_number_position(&navigated_entries, 3);
    let focused = context.memory(|memory| memory.focused()).unwrap();
    assert!(
        context
            .read_response(focused)
            .is_some_and(|response| response.rect.contains(selected_position))
    );

    // Act: Press Return to queue FIND/OLD/RUN for the selected ASCII PROG.
    send_app_key(&mut app, &context, ready, egui::Key::Enter);

    // Assert: File 3's selected launch is accepted and begins its queued command sequence.
    assert!(app.running);
    assert_eq!(app.program_launch_file, Some(3));
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);

    // Act: Advance in small frames so each BASIC prompt event can release the next line.
    let mut now = ready;
    for _ in 0..1_000 {
        now += Duration::from_millis(10);
        app_frame(&mut app, &context, now, vec![]);
        if app.input_sequence.auto_load_state() == AutoLoadState::Sent {
            break;
        }
    }

    // Assert: The complete FIND/OLD/RUN sequence finishes after its readiness transitions.
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Sent);

    // Act: Let the loaded BASIC program run and capture its visible output.
    let text = app_frame(&mut app, &context, now + Duration::from_secs(2), vec![]);
    assert!(
        text.iter().any(|(text, _)| {
            text == "Launch commands sent for file 3; execution may still fail"
        }),
        "missing launch status: {text:?}"
    );

    let mut expected = Machine::default();
    expected.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut expected);
    runtime
        .advance(&mut expected, Duration::from_secs(20))
        .unwrap();
    for &key in b"10\x10PRINT\x101\rRUN\r" {
        expected.keyboard_press(key);
        runtime
            .advance(&mut expected, Duration::from_millis(100))
            .unwrap();
        expected.keyboard_release();
        runtime
            .advance(&mut expected, Duration::from_millis(100))
            .unwrap();
    }
    runtime
        .advance(&mut expected, Duration::from_secs(2))
        .unwrap();
    let actual_line = &app.display().pixels()
        [126 * CpuReferenceRenderer::WIDTH..140 * CpuReferenceRenderer::WIDTH];
    let expected_line = &runtime.display(&mut expected).pixels()
        [104 * CpuReferenceRenderer::WIDTH..118 * CpuReferenceRenderer::WIDTH];
    assert!(
        actual_line
            .iter()
            .zip(expected_line)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
}

// What: Double-click a program entry after the emulator detects BASIC's cursor.
// Why: Mouse activation must queue the same selected-program commands as keyboard activation.
#[test]
fn flash_directory_double_click_launches_confirmed_ascii_program() {
    // Arrange: Start BASIC, confirm the visible prompt and open a directory containing an ASCII PROG.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "2      ASCII   PROG [Mouse] 12.txt".to_owned(),
            bytes: b"10 PRINT 1\r\r".to_vec(),
        }]),
        [Ok(())]
    );
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    app_frame(&mut app, &context, start, vec![]);
    click_app_control(&mut app, &context, start, "Start");
    let mut ready = start;
    for _ in 0..3_000 {
        if app.basic_prompt_ready {
            break;
        }
        ready += Duration::from_millis(10);
        app_frame(&mut app, &context, ready, vec![]);
    }
    assert!(app.basic_prompt_ready, "the ROM must draw its BASIC cursor");
    click_app_control(&mut app, &context, ready, "Show directory");
    let entries = app_frame(&mut app, &context, ready, vec![]);
    let position = directory_number_position(&entries, 2);

    // Act: Double-click the ASCII PROG number using distinct press/release frames.
    let launched = double_click_app_position_at_time(&mut app, &context, ready, position, 100.0);

    // Assert: The selected program's command sequence is queued without another keyboard gesture.
    assert!(app.running);
    assert_eq!(app.program_launch_file, Some(2));
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
    assert!(
        launched
            .iter()
            .any(|(text, _)| text == "Launching Flash Drive program 2")
    );
}

// What: Double-click the supplied binary FlyBy program from the Flash Drive directory.
// Why: BINARY PROG entries must use the shared BOLD launch path, not be rejected as non-ASCII.
#[test]
fn flash_directory_double_click_launches_confirmed_binary_program() {
    // Arrange: Start BASIC and import the user's binary program fixture.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "2      BINARY  PROG FlyBy simulator 28928".to_owned(),
            bytes: include_bytes!(
                "../../../web/FlashDrive/Opt30Demo/2      BINARY  PROG FlyBy simulator 28928"
            )
            .to_vec(),
        }]),
        [Ok(())]
    );
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    app_frame(&mut app, &context, start, vec![]);
    click_app_control(&mut app, &context, start, "Start");
    let mut ready = start;
    for _ in 0..3_000 {
        if app.basic_prompt_ready {
            break;
        }
        ready += Duration::from_millis(10);
        app_frame(&mut app, &context, ready, vec![]);
    }
    assert!(app.basic_prompt_ready, "the ROM must draw its BASIC cursor");
    click_app_control(&mut app, &context, ready, "Show directory");
    let entries = app_frame(&mut app, &context, ready, vec![]);
    let position = directory_number_position(&entries, 2);

    // Act: Double-click the binary program entry.
    let launched = double_click_app_position_at_time(&mut app, &context, ready, position, 100.0);

    // Assert: The selected binary program enters the shared keyboard sequencer without a notice.
    assert_eq!(app.program_launch_file, Some(2));
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
    assert!(app.program_launch_notice.is_none());
    assert!(
        launched
            .iter()
            .any(|(text, _)| text == "Launching Flash Drive program 2")
    );

    // Act: Advance the directory-launched script until its second command starts.
    let mut now = ready;
    let mut command_key = app.machine.read_byte(0x87a8).unwrap() & 0x7f;
    for _ in 0..3_000 {
        if command_key == b'C' {
            break;
        }
        now += Duration::from_millis(10);
        app_frame(&mut app, &context, now, vec![]);
        command_key = app.machine.read_byte(0x87a8).unwrap() & 0x7f;
    }

    // Assert: A BINARY PROG launch enters CALL "BOLD",5 rather than ASCII OLD@5:.
    assert_eq!(command_key, b'C');
}

// What: Pause BASIC after automatic cursor detection and attempt a directory launch.
// Why: Prompt readiness must be invalidated when execution stops, preventing queued commands from being stranded.
#[test]
fn flash_directory_pause_clears_automatic_readiness_before_launch() {
    // Arrange: Start with a stopped emulator, then let its ROM draw the transient cursor.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "2      ASCII   PROG [Paused] 12.txt".to_owned(),
            bytes: b"10 PRINT 1\r\r".to_vec(),
        }]),
        [Ok(())]
    );
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    app_frame(&mut app, &context, start, vec![]);
    click_app_control(&mut app, &context, start, "Show directory");
    assert!(!app.basic_prompt_ready);
    click_app_control(&mut app, &context, start, "Start");
    let mut ready = start;
    for _ in 0..3_000 {
        if app.basic_prompt_ready {
            break;
        }
        ready += Duration::from_millis(10);
        app_frame(&mut app, &context, ready, vec![]);
    }
    assert!(app.basic_prompt_ready, "the ROM must draw its BASIC cursor");
    click_app_control(&mut app, &context, ready, "Pause");

    // Act: Double-click the same ASCII PROG while paused after readiness was cleared.
    let paused_entries = app_frame(&mut app, &context, ready, vec![]);
    let position = directory_number_position(&paused_entries, 2);
    let result = double_click_app_position_at_time(&mut app, &context, ready, position, 100.0);

    // Assert: No launch is queued and the user is told to start and wait again.
    assert!(!app.running);
    assert!(!app.basic_prompt_ready);
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Idle);
    assert!(
        result.iter().any(|(text, _)| {
            text == "Start the emulator and wait for the BASIC prompt before launching"
        }),
        "missing paused-launch message: {result:?}"
    );
}

// What: Keep directory activation UI-owned, but deliver keys after focus returns to the machine.
// Why: UI ownership and BASIC readiness must not suppress actual machine-targeted keyboard input.
#[test]
fn flash_directory_keyboard_owns_ui_events_but_routes_machine_keys() {
    // Arrange: Open the directory and start an Auto Load press before navigating its controls.
    for key in [egui::Key::Space, egui::Key::Enter] {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut machine = Machine::default();
        machine.reset().unwrap();
        machine.write_byte(0x87a9, 0x04).unwrap();
        machine.write_byte(0x87ab, 0x04).unwrap();
        let mut app = Tek4051App::new(&creation, machine);
        let now = app.last_update;
        app_frame(&mut app, &context, now, vec![]);
        click_app_control(&mut app, &context, now, "Show directory");
        assert!(app.show_flash_directory);
        let directory_position = app_frame(&mut app, &context, now, vec![])
            .into_iter()
            .find_map(|(text, position)| (text == "Show directory").then_some(position))
            .unwrap();
        click_app_control(&mut app, &context, now, "Start");
        let now = advance_until_prompt(&mut app, &context, now);
        click_app_control(&mut app, &context, now, "Auto Load");
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
        assert!(!app.basic_prompt_ready);
        let held_macro_key = app.machine.read_byte(0x87a8).unwrap();

        // Act: Use actual Tab navigation while the macro is active, not a direct focus assignment.
        let mut directory_focus = None;
        for _ in 0..64 {
            for pressed in [true, false] {
                app_frame(
                    &mut app,
                    &context,
                    now,
                    vec![Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
                assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
                assert_eq!(app.machine.read_byte(0x87a8).unwrap(), held_macro_key);
            }
            if let Some(focused) = context.memory(|memory| memory.focused())
                && context
                    .read_response(focused)
                    .is_some_and(|response| response.rect.contains(directory_position))
            {
                directory_focus = Some(focused);
                break;
            }
        }
        let directory_focus =
            directory_focus.expect("directory checkbox did not receive Tab focus");
        let event = |pressed, repeat| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers: Modifiers::NONE,
        };
        let mut press = vec![event(true, false)];
        if key == egui::Key::Space {
            press.push(Event::Text(" ".to_owned()));
        }

        // Act: Keyboard-toggle the checkbox, then move focus before repeat and release events.
        app_frame(&mut app, &context, now, press);
        // Assert: Native activation changes only the view; the scripted key remains held.
        assert!(!app.show_flash_directory);
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
        assert_eq!(app.machine.read_byte(0x87a8).unwrap(), held_macro_key);
        // Act: Shift-Tab to Reset after closing the directory, with the activation key still held.
        let reset_position = app_frame(&mut app, &context, now, vec![])
            .into_iter()
            .find_map(|(text, position)| (text == "Reset").then_some(position))
            .unwrap();
        let mut reset_focused = false;
        for _ in 0..64 {
            for pressed in [true, false] {
                app_frame(
                    &mut app,
                    &context,
                    now,
                    vec![Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Modifiers {
                            shift: true,
                            ..Modifiers::NONE
                        },
                    }],
                );
                assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
                assert_eq!(app.machine.read_byte(0x87a8).unwrap(), held_macro_key);
            }
            if let Some(focused) = context.memory(|memory| memory.focused())
                && context
                    .read_response(focused)
                    .is_some_and(|response| response.rect.contains(reset_position))
            {
                reset_focused = true;
                break;
            }
        }
        assert!(reset_focused, "Reset did not receive Shift-Tab focus");
        assert_ne!(
            context.memory(|memory| memory.focused()),
            Some(directory_focus)
        );
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
        assert_eq!(app.machine.read_byte(0x87a8).unwrap(), held_macro_key);
        let mut repeat = vec![event(true, true)];
        if key == egui::Key::Space {
            repeat.push(Event::Text(" ".to_owned()));
        }
        app_frame(&mut app, &context, now, repeat);
        app_frame(&mut app, &context, now, vec![event(false, false)]);
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
        assert_eq!(app.machine.read_byte(0x87a8).unwrap(), held_macro_key);

        // Act: Return focus to the emulator, then press the same key before readiness returns.
        if let Some(focused) = context.memory(|memory| memory.focused()) {
            context.memory_mut(|memory| memory.surrender_focus(focused));
        }
        app_frame(&mut app, &context, now, vec![]);
        assert!(!app.basic_prompt_ready);
        app_frame(&mut app, &context, now, vec![event(true, false)]);
        // Assert: The physical key reaches the machine and cancels the scripted sequence.
        let expected_code = match key {
            egui::Key::Space => 0x10,
            egui::Key::Enter => b'\r',
            _ => unreachable!(),
        };
        assert_eq!(app.machine.read_byte(0x87a8).unwrap() & 0x7f, expected_code);
        assert_eq!(
            app.input_sequence.auto_load_state(),
            AutoLoadState::Cancelled
        );
        app_frame(&mut app, &context, now, vec![event(false, false)]);
        assert_eq!(
            app.input_sequence.auto_load_state(),
            AutoLoadState::Cancelled
        );
        assert_eq!(app.machine.read_byte(0x87a8).unwrap() & 0x7f, expected_code);
    }
}

// What: Deliver BASIC Tab while Auto Load has temporarily cleared prompt readiness.
// Why: Prompt readiness must not drop a host event addressed to the machine keyboard.
#[test]
fn flash_directory_hidden_without_native_focus_delivers_tab_before_prompt_ready() {
    // Arrange: Start Auto Load with the optional directory hidden and no focused native widget.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    app_frame(&mut app, &context, now, vec![]);
    click_app_control(&mut app, &context, now, "Start");
    let now = advance_until_prompt(&mut app, &context, now);
    click_app_control(&mut app, &context, now, "Auto Load");
    assert!(!app.show_flash_directory);
    assert!(!app.basic_prompt_ready);
    assert!(context.memory(|memory| memory.focused()).is_none());
    let held_macro_key = app.machine.read_byte(0x87a8).unwrap();

    // Act: Send Tab to the machine while the scripted command has cleared readiness.
    app_frame(
        &mut app,
        &context,
        now,
        vec![Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );

    // Assert: The machine receives Tab and the physical key interrupts Auto Load's sequence.
    assert_eq!(app.machine.read_byte(0x87a8).unwrap() & 0x7f, 0x09);
    assert_ne!(app.machine.read_byte(0x87a8).unwrap(), held_macro_key);
    assert_eq!(
        app.input_sequence.auto_load_state(),
        AutoLoadState::Cancelled
    );
    assert!(context.memory(|memory| memory.focused()).is_none());
    app_frame(
        &mut app,
        &context,
        now,
        vec![Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(
        app.input_sequence.auto_load_state(),
        AutoLoadState::Cancelled
    );
    assert_eq!(app.machine.read_byte(0x87a8).unwrap() & 0x7f, 0x09);
}

fn export_frame(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
    export: &mut impl FnMut(FlashExportRequest<'_>) -> Result<Option<PathBuf>, String>,
    clock: &mut impl FnMut() -> Instant,
) -> Vec<String> {
    context
        .run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
                events,
                ..Default::default()
            },
            |context| {
                app.update_at_with_dialogs(
                    context,
                    now,
                    &mut |_| Ok(None),
                    &mut || None,
                    export,
                    clock,
                )
            },
        )
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

fn archive_export_frame(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
    archive_export: &mut impl FnMut(&[u8]) -> Result<Option<PathBuf>, String>,
    clock: &mut impl FnMut() -> Instant,
) -> Vec<String> {
    let mut export = |request: FlashExportRequest<'_>| match request {
        FlashExportRequest::File(_) => Ok(None),
        FlashExportRequest::Archive(machine) => {
            let archive = machine.write_flash_storage_archive(Cursor::new(Vec::new()))?;
            archive_export(&archive.into_inner())
        }
    };
    export_frame(app, context, now, events, &mut export, clock)
}

fn archive_import_frame(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
    confirm_replacement: &mut impl FnMut() -> bool,
    pick_archive: &mut impl FnMut() -> Option<PathBuf>,
    clock: &mut impl FnMut() -> Instant,
) -> Vec<String> {
    context
        .run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
                events,
                ..Default::default()
            },
            |context| {
                let mut merge_import = || None;
                let mut imports = FlashImportDialogs {
                    files: &mut merge_import,
                    storage_archive_confirm: confirm_replacement,
                    storage_archive_pick: pick_archive,
                };
                app.update_at_with_import_dialogs(
                    context,
                    now,
                    &mut |_| Ok(None),
                    &mut imports,
                    &mut |_| Ok(None),
                    clock,
                )
            },
        )
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

// What: Drive whole-storage Save As through its native control and observable outcomes.
// Why: Archive export must write a valid ZIP, report cancel/errors, and account for modal time.
#[test]
fn flash_storage_archive_export_control_saves_and_reports_outcomes() {
    // Arrange: Exercise Save As success, cancellation, write failure, and empty storage.
    for outcome in ["save", "cancel", "write error", "empty"] {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut machine = Machine::default();
        machine.reset().unwrap();
        if outcome != "empty" {
            assert_eq!(
                machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
                    name: "7      ASCII   PROG Archive 999".to_owned(),
                    bytes: b"10 END\r".to_vec(),
                }]),
                [Ok(())]
            );
        }
        let mut app = Tek4051App::new(&creation, machine);
        let now = app.last_update;
        let after_dialog = now + Duration::from_secs(5);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Archive.zip");
        let mut calls = 0;
        let mut archive_export = |archive: &[u8]| {
            calls += 1;
            match outcome {
                "save" => {
                    std::fs::write(&path, archive).unwrap();
                    Ok(Some(path.clone()))
                }
                "cancel" => Ok(None),
                _ => Err("disk failure".to_owned()),
            }
        };
        let mut clock_calls = 0;
        let mut clock = || {
            clock_calls += 1;
            after_dialog
        };
        app_frame(&mut app, &context, now, vec![]);
        let position = app_frame(&mut app, &context, now, vec![])
            .into_iter()
            .find_map(|(text, position)| {
                (text == "Export Flash Drive archive...").then_some(position)
            })
            .unwrap();

        // Act: Click the archive button with an injected Save As handler.
        for pressed in [true, false] {
            archive_export_frame(
                &mut app,
                &context,
                now,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                &mut archive_export,
                &mut clock,
            );
        }
        let dialog_update = app.last_update;
        let painted = archive_export_frame(
            &mut app,
            &context,
            dialog_update,
            vec![],
            &mut archive_export,
            &mut clock,
        );

        // Assert: Notice, dialog timing and storage state reflect the selected outcome.
        let (error, notice) = app.export_notice.as_ref().unwrap();
        assert_eq!(*error, matches!(outcome, "write error" | "empty"));
        assert!(painted.iter().any(|text| text == notice));
        assert_eq!(calls, usize::from(outcome != "empty"));
        assert_eq!(clock_calls, usize::from(outcome != "empty"));
        assert_eq!(
            dialog_update,
            if outcome == "empty" {
                now
            } else {
                after_dialog
            }
        );
        assert!(!app.running);
        if outcome == "save" {
            let file = File::open(&path).unwrap();
            let mut archive = ZipArchive::new(file).unwrap();
            assert_eq!(archive.len(), 2);
            assert_eq!(archive.by_index(0).unwrap().name(), "0001");
            let mut contents = Vec::new();
            archive
                .by_index(1)
                .unwrap()
                .read_to_end(&mut contents)
                .unwrap();
            assert_eq!(contents, b"10 END\r");
        } else if outcome == "cancel" {
            assert!(notice.contains("cancelled"));
        } else {
            assert!(notice.contains(if outcome == "empty" {
                "contains no files"
            } else {
                "disk failure"
            }));
        }
    }
}

// What: Drive confirmation, picker cancellation, replacement, and import failure through the native control.
// Why: Declining confirmation must skip the picker, and only a validated archive may replace storage.
#[test]
fn flash_storage_archive_import_control_reports_all_outcomes() {
    // Arrange: Prepare a valid replacement archive and retain file 9 as the unchanged baseline.
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("header", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(br#"[2,"A","P","New"]"#).unwrap();
    writer
        .start_file("content", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"10 END\r").unwrap();
    let archive_bytes = writer.finish().unwrap().into_inner();

    for outcome in ["success", "declined", "picker cancel", "failure"] {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut machine = Machine::default();
        machine.reset().unwrap();
        assert_eq!(
            machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "9      ASCII   PROG Previous 999".to_owned(),
                bytes: b"old\r".to_vec(),
            }]),
            [Ok(())]
        );
        let before = machine.flash_drive_directory();
        let mut app = Tek4051App::new(&creation, machine);
        let now = app.last_update;
        let after_dialog = now + Duration::from_secs(5);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("storage.zip");
        if outcome == "success" {
            std::fs::write(&path, &archive_bytes).unwrap();
        } else if outcome == "failure" {
            std::fs::write(&path, b"not a ZIP archive").unwrap();
        }
        let mut confirmation_calls = 0;
        let mut confirm_replacement = || {
            confirmation_calls += 1;
            outcome != "declined"
        };
        let mut picker_calls = 0;
        let mut pick_archive = || {
            picker_calls += 1;
            (outcome != "picker cancel").then(|| path.clone())
        };
        let mut clock_calls = 0;
        let mut clock = || {
            clock_calls += 1;
            after_dialog
        };
        app_frame(&mut app, &context, now, vec![]);
        let rendered = app_frame(&mut app, &context, now, vec![]);
        let position = rendered
            .iter()
            .find_map(|(label, position)| {
                (label == "Import Flash Drive archive...").then_some(*position)
            })
            .unwrap_or_else(|| panic!("replacement control not rendered: {rendered:?}"));

        // Act: Click the replacement control using separate injected confirmation and picker decisions.
        for pressed in [true, false] {
            archive_import_frame(
                &mut app,
                &context,
                now,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                &mut confirm_replacement,
                &mut pick_archive,
                &mut clock,
            );
        }
        let dialog_update = app.last_update;
        let painted = archive_import_frame(
            &mut app,
            &context,
            dialog_update,
            vec![],
            &mut confirm_replacement,
            &mut pick_archive,
            &mut clock,
        );

        // Assert: Confirmation controls picker invocation, failures preserve storage, and each outcome is visible.
        assert_eq!(confirmation_calls, 1);
        assert_eq!(picker_calls, usize::from(outcome != "declined"));
        assert_eq!(clock_calls, 1);
        assert_eq!(dialog_update, after_dialog);
        assert_eq!(app.import_notices.len(), 1);
        let (error, notice) = &app.import_notices[0];
        assert_eq!(*error, outcome == "failure");
        assert!(painted.iter().any(|text| text == notice));
        if outcome == "success" {
            assert!(notice.contains("storage.zip (1 file(s))"));
            assert_eq!(app.machine.flash_drive_directory()[0].number, 2);
        } else {
            assert_eq!(app.machine.flash_drive_directory(), before);
            assert!(
                notice.contains(if matches!(outcome, "declined" | "picker cancel") {
                    "cancelled"
                } else {
                    "invalid ZIP archive"
                })
            );
        }
    }
}

// What: Write an archive through the same file-backed adapter used by native Save As.
// Why: Streaming to a filesystem file must produce a readable ZIP without an in-memory archive buffer.
#[test]
fn flash_storage_archive_path_writer_creates_readable_zip() {
    // Arrange: Store one program and choose a new destination inside a temporary directory.
    let mut machine = Machine::default();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "7      ASCII   PROG Archive 999".to_owned(),
            bytes: b"10 END\r".to_vec(),
        }]),
        [Ok(())]
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Archive.zip");

    // Act: Run the production path writer, which creates the file and streams the ZIP into it.
    let session = super::EmulationState::new(machine);
    write_flash_storage_archive_to_path(&path, &session).unwrap();

    // Assert: The resulting file opens as a ZIP with the expected header and unchanged content.
    let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
    assert_eq!(archive.len(), 2);
    let mut header = String::new();
    archive
        .by_name("0001")
        .unwrap()
        .read_to_string(&mut header)
        .unwrap();
    assert_eq!(header, r#"[7,"A","P","Archive         "]"#);
    let mut content = Vec::new();
    archive
        .by_name("0002")
        .unwrap()
        .read_to_end(&mut content)
        .unwrap();
    assert_eq!(content, b"10 END\r");
}

// What: Click numbered export while paused, covering save, cancellation, I/O error and missing entry.
// Why: The real control must export the selected canonical bytes and surface every outcome.
#[test]
fn flash_export_control_saves_bytes_and_reports_all_outcomes() {
    // Arrange: Import a binary entry, select its number, and prepare a workspace-independent temp destination.
    for outcome in ["save", "cancel", "write error", "missing"] {
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut machine = Machine::default();
        machine.reset().unwrap();
        let bytes = vec![0, 0xff, 13, 0x7f];
        assert_eq!(
            machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
                name: "7      BINARY  DATA Export 999".to_owned(),
                bytes: bytes.clone(),
            }]),
            [Ok(())]
        );
        let mut app = Tek4051App::new(&creation, machine);
        app.export_number = if outcome == "missing" { 8 } else { 7 };
        let now = app.last_update;
        let directory = tempfile::tempdir().unwrap();
        let canonical = app.machine.export_flash_file(7).unwrap();
        let path = directory.path().join(&canonical.name);
        let mut calls = 0;
        let mut export = |request: FlashExportRequest<'_>| match request {
            FlashExportRequest::File(file) => {
                calls += 1;
                assert_eq!(file.name, canonical.name);
                assert_eq!(file.bytes, bytes);
                match outcome {
                    "save" => {
                        write_flash_file(&path, file)?;
                        Ok(Some(path.clone()))
                    }
                    "cancel" => Ok(None),
                    _ => Err("disk failure".to_owned()),
                }
            }
            FlashExportRequest::Archive(_) => panic!("unexpected archive export"),
        };
        let mut clock_calls = 0;
        let mut clock = || {
            clock_calls += 1;
            now
        };
        app_frame(&mut app, &context, now, vec![]);
        let position = app_frame(&mut app, &context, now, vec![])
            .into_iter()
            .find_map(|(text, position)| (text == "Export Flash Drive file...").then_some(position))
            .unwrap();

        // Act: Use the painted button with an injected Save As adapter, not a direct app-method call.
        for pressed in [true, false] {
            export_frame(
                &mut app,
                &context,
                now,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                &mut export,
                &mut clock,
            );
        }
        let text = export_frame(&mut app, &context, now, vec![], &mut export, &mut clock);

        // Assert: Save writes exact raw bytes/numbered metadata; every failure/cancellation is painted.
        let (error, notice) = app.export_notice.as_ref().unwrap();
        assert_eq!(*error, matches!(outcome, "write error" | "missing"));
        assert!(text.iter().any(|text| text == notice));
        assert_eq!(calls, usize::from(outcome != "missing"));
        assert_eq!(clock_calls, usize::from(outcome != "missing"));
        assert!(!app.running);
        if outcome == "save" {
            let restored = read_flash_file(&path).unwrap();
            assert_eq!(restored.name, canonical.name);
            assert_eq!(restored.bytes, bytes);
        } else if outcome == "cancel" {
            assert!(notice.contains("cancelled"));
        } else {
            assert!(notice.contains(if outcome == "missing" {
                "does not exist"
            } else {
                "disk failure"
            }));
        }
        assert_eq!(app.machine.export_flash_file(7).unwrap().bytes, bytes);
    }
}

// What: Block export behind active Auto Load, then simulate a modal export with an erase pending.
// Why: Export must not interrupt scripted keys or accumulate modal CPU time, while erase still completes.
#[test]
fn flash_export_respects_auto_load_and_modal_timing() {
    // Arrange: Seed a running NOP machine with a detected cursor and settle the export control.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.store_tape_program(1, b"10 PRINT 1\r").unwrap();
    for offset in 0..5_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    click_app_control(&mut app, &context, now, "Start");
    observe_cursor_glyph(&mut app.backend.state);
    click_app_control(&mut app, &context, now, "Auto Load");
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, position)| (text == "Export Flash Drive file...").then_some(position))
        .unwrap();
    let mut calls = 0;
    let mut export = |request: FlashExportRequest<'_>| {
        assert!(matches!(request, FlashExportRequest::File(_)));
        calls += 1;
        Ok(None)
    };
    let after_dialog = now + Duration::from_secs(5);
    let mut clock = || after_dialog;

    // Act: Attempt export during typing, then cancel the macro and repeat with an erase pending.
    for pressed in [true, false] {
        export_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            &mut export,
            &mut clock,
        );
    }
    assert!(app.export_notice.is_none());
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
    app.cancel_auto_load();
    app_frame(&mut app, &context, now, vec![]);
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, position)| (text == "Export Flash Drive file...").then_some(position))
        .unwrap();
    app.machine.erase_display();
    for pressed in [true, false] {
        export_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            &mut export,
            &mut clock,
        );
    }

    // Assert: One actual dialog occurred; its five seconds expired erase but did not execute NOPs.
    assert_eq!(calls, 1);
    assert_eq!(app.machine.registers().pc, 0x0100);
    assert!(!app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(0));
    export_frame(
        &mut app,
        &context,
        after_dialog + Duration::from_millis(10),
        vec![],
        &mut |_| Ok(None),
        &mut clock,
    );
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264);
    assert!(app.runtime_error.is_none());
}

// What: Write exported bytes to an invalid destination instead of a regular file.
// Why: Filesystem errors must reach the export adapter rather than pretending a save succeeded.
#[test]
fn flash_export_writer_reports_destination_errors() {
    // Arrange: Use an existing directory as the invalid file destination.
    let directory = tempfile::tempdir().unwrap();
    let file = tek4051_emulator::machine::gpib::FlashDriveFile {
        name: "file".to_owned(),
        bytes: vec![0xff],
    };
    // Act: Attempt the raw-byte write used by native Save As.
    let error = write_flash_file(directory.path(), &file).unwrap_err();
    // Assert: The diagnostic reports the failed write and its path.
    assert!(error.contains("cannot write"));
    assert!(error.contains(&directory.path().display().to_string()));
}

// What: Edit the export number without typing into BASIC or leaving an emulator key held.
// Why: Native text focus owns digits and Enter, while front-panel pointer input must still work.
#[test]
fn flash_export_number_editing_owns_keyboard_but_not_panel_pointer_input() {
    // Arrange: Expose keyboard input pins and locate the settled native number field.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    app_frame(&mut app, &context, start, vec![]);
    click_app_control(&mut app, &context, start, "Start");
    let now = advance_until_prompt(&mut app, &context, start);
    app_frame(&mut app, &context, now, vec![]);
    let text = app_frame(&mut app, &context, now, vec![]);
    let number_position = text
        .iter()
        .find_map(|(text, position)| (text == "1").then_some(*position))
        .unwrap();
    let panel_position = text
        .iter()
        .find_map(|(text, position)| (text == "HOME / PAGE").then_some(*position))
        .unwrap();
    let modifiers = Modifiers {
        shift: true,
        ctrl: true,
        ..Modifiers::NONE
    };
    let frame = |app: &mut Tek4051App, events| {
        let _ = context.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
                modifiers,
                events,
                ..Default::default()
            },
            |context| app.update_at(context, now),
        );
    };
    frame(
        &mut app,
        vec![
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            Event::Text("A".to_owned()),
        ],
    );
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0xc1);
    assert_eq!(app.machine.read_byte(0x87aa).unwrap() & 0x03, 0);

    // Act: Focus the number editor while A is held, replace its value, and commit with Enter.
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![
                Event::PointerMoved(number_position),
                Event::PointerButton {
                    pos: number_position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
        );
    }
    assert!(app.keyboard.is_idle());
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(app.machine.read_byte(0x87aa).unwrap() & 0x03, 0x03);
    frame(
        &mut app,
        vec![
            Event::Key {
                key: egui::Key::Num7,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            Event::Text("7".to_owned()),
        ],
    );
    assert_eq!(app.export_number, 7);
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0x80);
    frame(
        &mut app,
        vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
    );

    // Assert: Commit does not send RETURN; clicking a panel key after refocusing still sends its code.
    assert!(app.keyboard.is_idle());
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0x80);
    assert_eq!(app.machine.read_byte(0x87aa).unwrap() & 0x03, 0x03);
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![
                Event::PointerMoved(number_position),
                Event::PointerButton {
                    pos: number_position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
        );
    }
    frame(
        &mut app,
        vec![
            Event::PointerMoved(panel_position),
            Event::PointerButton {
                pos: panel_position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            },
        ],
    );
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0xea);

    // Act: Keep HOME held beyond the minimum observation interval, then release and finish its gap.
    app_frame(&mut app, &context, now + Duration::from_millis(150), vec![]);
    // Assert: A later frame must not cancel the panel press merely because editing just lost focus.
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0xea);
    assert!(!app.keyboard.is_idle());
    app_frame(
        &mut app,
        &context,
        now + Duration::from_millis(160),
        vec![Event::PointerButton {
            pos: panel_position,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(app.machine.read_byte(0x87a8).unwrap(), 0x80);
    app_frame(&mut app, &context, now + Duration::from_millis(300), vec![]);
    assert!(app.keyboard.is_idle());
    assert_eq!(app.machine.read_byte(0x87aa).unwrap() & 0x03, 0x03);
}

// What: Auto Load stays disabled while paused and cancels on controls, errors, or delayed focus loss.
// Why: The native macro must not start execution implicitly or keep sending keys after interruption.
#[test]
fn auto_load_control_requires_running_and_reports_interruptions() {
    for interruption in ["Pause", "Reset", "runtime error", "focus loss"] {
        // Arrange: Create the normally paused native app and settle its initial control layout.
        let context = egui::Context::default();
        let creation = eframe::CreationContext::_new_kittest(context.clone());
        let mut machine = Machine::default();
        machine.reset().unwrap();
        let mut app = Tek4051App::new(&creation, machine);
        let now = app.last_update;
        let _ = app_frame(&mut app, &context, now, vec![]);
        let pc = app.machine.registers().pc;

        // Act: Attempt Auto Load while paused, then explicitly Start.
        click_app_control(&mut app, &context, now, "Auto Load");
        assert!(!app.running);
        assert_eq!(app.machine.registers().pc, pc);
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Idle);
        click_app_control(&mut app, &context, now, "Start");
        let now = advance_until_prompt(&mut app, &context, now);

        // Act: Attempt Auto Load while a manual host key owns the keyboard.
        let host_key = |pressed| Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        app_frame(&mut app, &context, now, vec![host_key(true)]);
        click_app_control(&mut app, &context, now, "Auto Load");
        let text = app_frame(&mut app, &context, now, vec![]);

        // Assert: Busy input is visibly rejected without displacing the user's held key.
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Idle);
        assert!(!app.keyboard.is_idle());
        assert!(
            text.iter()
                .any(|(text, _)| text == "Auto Load: keyboard input is busy")
        );

        // Act: Release the manual key and start a fresh macro.
        app_frame(&mut app, &context, now, vec![host_key(false)]);
        let now = now + Duration::from_millis(200);
        app_frame(&mut app, &context, now, vec![]);
        click_app_control(&mut app, &context, now, "Auto Load");
        assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Typing);
        assert!(app.auto_load_error.is_none());

        // Act: Interrupt through a control, a runtime failure, or a delayed unfocused frame.
        let after = if interruption == "runtime error" {
            app.backend.state.fail("injected runtime error".to_owned());
            let after = now + Duration::from_millis(10);
            app_frame(&mut app, &context, after, vec![]);
            assert!(app.runtime_error.is_some());
            after
        } else if interruption == "focus loss" {
            let after = now + Duration::from_secs(6);
            let _ = context.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
                    focused: false,
                    ..Default::default()
                },
                |context| app.update_at(context, after),
            );
            after
        } else {
            click_app_control(&mut app, &context, now, interruption);
            now
        };

        // Assert: Even a frame longer than the macro cancels keys; reset/focus loss retain execution.
        let text = app_frame(&mut app, &context, after, vec![]);
        assert_eq!(
            app.input_sequence.auto_load_state(),
            AutoLoadState::Cancelled
        );
        assert!(app.keyboard.is_idle());
        assert_eq!(app.running, matches!(interruption, "Reset" | "focus loss"));
        assert!(
            text.iter()
                .any(|(text, _)| text.contains("Auto Load cancelled"))
        );
    }
}

// What: Click native Auto Load and run an imported file-1 program through the real ROM.
// Why: Command delivery must produce the correct output, not merely drain the native key queue.
#[test]
fn auto_load_control_runs_imported_program_through_rom() {
    // Arrange: Import a small ASCII program and explicitly boot the running native machine.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    assert_eq!(
        machine.import_flash_files(&[tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "1      ASCII   PROG [Auto] 12.txt".to_owned(),
            bytes: b"10 PRINT 1\r\r".to_vec(),
        }]),
        [Ok(())]
    );
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    let _ = app_frame(&mut app, &context, start, vec![]);
    click_app_control(&mut app, &context, start, "Start");
    let ready = advance_until_prompt(&mut app, &context, start);

    // Act: Click the real control and let the timed key queue execute FIND/OLD/RUN.
    click_app_control(&mut app, &context, ready, "Auto Load");
    let text = app_frame(&mut app, &context, ready + Duration::from_secs(6), vec![]);

    // Arrange: Independently enter the expected program through keyboard/ROM, without Auto Load.
    let mut expected = Machine::default();
    expected.reset().unwrap();
    let mut runtime = HostRuntime::start(&mut expected);
    runtime
        .advance(&mut expected, Duration::from_secs(20))
        .unwrap();
    for &key in b"10\x10PRINT\x101\rRUN\r" {
        expected.keyboard_press(key);
        runtime
            .advance(&mut expected, Duration::from_millis(100))
            .unwrap();
        expected.keyboard_release();
        runtime
            .advance(&mut expected, Duration::from_millis(100))
            .unwrap();
    }
    runtime
        .advance(&mut expected, Duration::from_secs(2))
        .unwrap();

    // Assert: The complete stored-pixel output shape/position matches independently entered BASIC.
    let actual_line = &app.display().pixels()
        [126 * CpuReferenceRenderer::WIDTH..140 * CpuReferenceRenderer::WIDTH];
    let expected_line = &runtime.display(&mut expected).pixels()
        [104 * CpuReferenceRenderer::WIDTH..118 * CpuReferenceRenderer::WIDTH];
    assert!(actual_line.iter().any(|&pixel| pixel >= 240));
    assert!(
        actual_line
            .iter()
            .zip(expected_line)
            .all(|(&actual, &expected)| (actual >= 240) == (expected >= 240))
    );
    assert_eq!(app.input_sequence.auto_load_state(), AutoLoadState::Sent);
    assert!(app.runtime_error.is_none());
    assert!(app.running);
    assert!(
        text.iter()
            .any(|(text, _)| text == "Auto Load commands sent (file 1)")
    );
}

fn import_frame(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
    import: &mut impl FnMut() -> Option<FlashImportSelection>,
    clock: &mut impl FnMut() -> Instant,
) -> Vec<String> {
    context
        .run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
                events,
                ..Default::default()
            },
            |context| {
                app.update_at_with_dialogs(
                    context,
                    now,
                    &mut |_| Ok(None),
                    import,
                    &mut |_| Ok(None),
                    clock,
                );
            },
        )
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

fn click_import(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    position: Pos2,
    import: &mut impl FnMut() -> Option<FlashImportSelection>,
    clock: &mut impl FnMut() -> Instant,
) {
    for pressed in [true, false] {
        import_frame(
            app,
            context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            import,
            clock,
        );
    }
}

// What: Import a mixed selection, report every failure, then cancel another native dialog.
// Why: The actual egui control must retain partial successes without claiming all files succeeded.
#[test]
fn flash_import_button_reports_partial_results_and_cancellation_without_starting() {
    // Arrange: Supply one valid file, one invalid filename, and an independent host read error.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let pc = app.machine.registers().pc;
    let _ = app_frame(&mut app, &context, now, vec![]);
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, pos)| (text == "Import folder contents...").then_some(pos))
        .unwrap();
    let mut selected_files = FlashImportSelection::default();
    selected_files.push_file(tek4051_emulator::machine::gpib::FlashDriveFile {
        name: "1      ASCII   PROG [ROM] 0".to_owned(),
        bytes: b"10 END\r\r".to_vec(),
    });
    selected_files.push_error("Import failed for unreadable.txt: read denied".to_owned());
    selected_files.push_file(tek4051_emulator::machine::gpib::FlashDriveFile {
        name: "bad name".to_owned(),
        bytes: vec![],
    });
    let mut selection = Some(selected_files);
    let mut import = || selection.take();
    let mut clock = || now;

    // Act: Click the real import button, then paint the resulting notices.
    click_import(&mut app, &context, now, position, &mut import, &mut clock);
    let text = import_frame(&mut app, &context, now, vec![], &mut import, &mut clock);

    // Assert: The core accepted one file, both distinct failures are visible, and execution is paused.
    assert!(
        text.iter()
            .any(|text| text == "1 Flash Drive file(s) imported")
    );
    assert!(
        text.iter()
            .any(|text| text.contains("bad name: invalid Flash Drive filename"))
    );
    assert!(
        text.iter()
            .any(|text| text.contains("unreadable.txt: read denied"))
    );
    assert_eq!(
        app.import_notices
            .iter()
            .filter(|(error, _)| *error)
            .count(),
        2
    );
    let errors: Vec<_> = app
        .import_notices
        .iter()
        .filter(|(error, _)| *error)
        .map(|(_, message)| message.as_str())
        .collect();
    assert_eq!(
        errors,
        [
            "Import failed for unreadable.txt: read denied",
            "Import failed for bad name: invalid Flash Drive filename"
        ]
    );
    assert!(!app.running);
    assert_eq!(app.machine.registers().pc, pc);

    // Act: Open the same control again; the fake dialog now returns cancellation.
    click_import(&mut app, &context, now, position, &mut import, &mut clock);
    let text = import_frame(&mut app, &context, now, vec![], &mut import, &mut clock);

    // Assert: Cancellation replaces the prior notices rather than claiming another import.
    assert_eq!(
        app.import_notices,
        [(false, "Flash Drive import cancelled".to_owned())]
    );
    assert!(
        text.iter()
            .any(|text| text == "Flash Drive import cancelled")
    );
    assert!(app.runtime_error.is_none());

    // Act: Select only an invalid filename and an independent host read failure.
    let mut failed_import = || {
        let mut selection = FlashImportSelection::default();
        selection.push_error("Import failed for unreadable.txt: read denied".to_owned());
        selection.push_file(tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "bad name".to_owned(),
            bytes: vec![],
        });
        Some(selection)
    };
    click_import(
        &mut app,
        &context,
        now,
        position,
        &mut failed_import,
        &mut clock,
    );
    let text = import_frame(
        &mut app,
        &context,
        now,
        vec![],
        &mut failed_import,
        &mut clock,
    );

    // Assert: Zero imported files and both failures are errors, not a success-shaped result.
    assert!(
        text.iter()
            .any(|text| text == "0 Flash Drive file(s) imported")
    );
    assert_eq!(app.import_notices.len(), 3);
    assert!(app.import_notices.iter().all(|(error, _)| *error));
}

// What: Spend five seconds in an import dialog while CPU intervals and an erase are pending.
// Why: Import must preserve the screen-copy modal rule: expire erase callbacks but exclude CPU time.
#[test]
fn flash_import_dialog_time_expires_erase_without_advancing_cpu_intervals() {
    // Arrange: Start a known NOP stream and provide an imported file through a fake dialog.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for offset in 0..5_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    click_app_control(&mut app, &context, now, "Start");
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, pos)| (text == "Import folder contents...").then_some(pos))
        .unwrap();
    app.machine.erase_display();
    let after_dialog = now + Duration::from_secs(5);
    let mut clock = || after_dialog;
    let mut import = || {
        let mut selection = FlashImportSelection::default();
        selection.push_file(tek4051_emulator::machine::gpib::FlashDriveFile {
            name: "1      ASCII   PROG [ROM] 0".to_owned(),
            bytes: b"10 END\r\r".to_vec(),
        });
        Some(selection)
    };

    // Act: Complete the import after five seconds of simulated modal time.
    click_import(&mut app, &context, now, position, &mut import, &mut clock);

    // Assert: Import succeeded, the erase completed, and no five-second CPU backlog ran.
    assert_eq!(
        app.import_notices,
        [(false, "1 Flash Drive file(s) imported".to_owned())]
    );
    assert!(!app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(0));
    assert_eq!(app.machine.registers().pc, 0x0100);
    assert!(app.running);

    // Act: Advance the first interval after the modal dialog closed.
    import_frame(
        &mut app,
        &context,
        after_dialog + Duration::from_millis(10),
        vec![],
        &mut import,
        &mut clock,
    );

    // Assert: Exactly one x1 instruction budget executes and the runtime remains healthy.
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264);
    assert!(app.runtime_error.is_none());
}

// What: Read raw host bytes under a Flash Drive basename and reject a missing selected file.
// Why: Native import must not decode file contents or hide filesystem errors.
#[test]
fn native_flash_file_reader_keeps_bytes_and_reports_read_errors() {
    // Arrange: Create a local binary fixture with a filename containing metadata spaces.
    let directory = tempfile::tempdir().unwrap();
    let name = "7      BINARY  DATA [Bytes] 999.bin";
    let path = directory.path().join(name);
    std::fs::write(&path, [0, 0xff, 0x80]).unwrap();

    // Act: Read the actual file and try an absent sibling.
    let file = read_flash_file(&path).unwrap();
    let missing = directory.path().join("missing.txt");
    let error = read_flash_file(&missing).expect_err("missing file unexpectedly read");

    // Assert: The basename and binary bytes are unmodified, and the failing path is explicit.
    assert_eq!(file.name, name);
    assert_eq!(file.bytes, [0, 0xff, 0x80]);
    assert!(error.contains(&missing.display().to_string()));
    assert!(error.starts_with("Import failed for "));
}

// What: Route loose files and ZIPs through one selected-path batch with a missing loose file.
// Why: Normal selection must combine both import paths in order while preserving individual failures.
#[test]
fn native_flash_path_selection_imports_loose_and_zip_files_and_reports_missing_path() {
    // Arrange: Prepare one binary file, one nested ZIP member, and an absent selected path.
    let directory = tempfile::tempdir().unwrap();
    let loose_name = "1      ASCII   PROG [Loose] 5.txt";
    let loose_path = directory.path().join(loose_name);
    std::fs::write(&loose_path, b"10 END").unwrap();
    let archive_path = directory.path().join("selected.zip");
    let archive_name = "2      BINARY  DATA [Archive] 2.dat";
    std::fs::write(
        &archive_path,
        create_flash_test_archive(&[(&format!("nested/{archive_name}"), &[0, 0xff])]),
    )
    .unwrap();
    let missing_path = directory.path().join("missing.txt");

    // Act: Process the loose path, ZIP path, and missing path in their selection order.
    let selection = read_flash_paths([loose_path, archive_path, missing_path.clone()]);

    // Assert: Both files retain their names and bytes; the failed path is reported separately.
    assert_eq!(
        selection
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>(),
        [loose_name, archive_name]
    );
    assert_eq!(selection.files[0].bytes, b"10 END");
    assert_eq!(selection.files[1].bytes, [0, 0xff]);
    assert_eq!(selection.errors.len(), 1);
    assert!(selection.errors[0].contains(&missing_path.display().to_string()));
    assert!(selection.errors[0].starts_with("Import failed for "));
}

// What: Import all direct files in a folder, including ZIP contents, but not nested-folder files.
// Why: Folder selection must cover the visible directory without recursively importing unrelated files.
#[test]
fn native_flash_folder_selection_imports_all_top_level_files() {
    // Arrange: Put loose files, a ZIP, and an otherwise valid file in a subfolder.
    let directory = tempfile::tempdir().unwrap();
    let first_name = "1      ASCII   PROG [First] 1.txt";
    let second_name = "2      ASCII   PROG [Second] 1.txt";
    std::fs::write(directory.path().join(second_name), b"20 END").unwrap();
    std::fs::write(directory.path().join(first_name), b"10 END").unwrap();
    let archive_name = "3      ASCII   PROG [Archive] 1.txt";
    let archive_path = directory.path().join("bundle.zip");
    std::fs::write(
        &archive_path,
        create_flash_test_archive(&[(&format!("nested/{archive_name}"), b"30 END")]),
    )
    .unwrap();
    let nested = directory.path().join("subfolder");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("4      ASCII   PROG [Nested] 1.txt"), b"40 END").unwrap();

    // Act: Import the selected folder as one ordered host-file batch.
    let selection = read_flash_directory(directory.path());

    // Assert: Direct loose files and ZIP contents are imported; nested-folder files are excluded.
    assert_eq!(
        selection
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>(),
        [first_name, second_name, archive_name]
    );
    assert_eq!(selection.files[0].bytes, b"10 END");
    assert_eq!(selection.files[1].bytes, b"20 END");
    assert_eq!(selection.files[2].bytes, b"30 END");
    assert!(selection.errors.is_empty());
}

// What: Reject empty and unreadable folder selections with explicit errors.
// Why: A selected folder that yields no files must not look like a successful import.
#[test]
fn native_flash_folder_selection_reports_empty_and_missing_directories() {
    // Arrange: Create one empty directory and name a second directory that does not exist.
    let parent = tempfile::tempdir().unwrap();
    let empty = parent.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let missing = parent.path().join("missing");

    // Act: Prepare each folder selection for import.
    let empty_selection = read_flash_directory(&empty);
    let missing_selection = read_flash_directory(&missing);

    // Assert: Both failure messages identify the user-selected directory.
    assert!(
        empty_selection
            .errors
            .iter()
            .any(|error| error.contains(&empty.display().to_string()))
    );
    assert!(
        missing_selection
            .errors
            .iter()
            .any(|error| error.contains(&missing.display().to_string()))
    );
}

// What: Keep readable files when one directory entry cannot be inspected.
// Why: A broken entry must be reported without discarding unrelated files from the same folder.
#[cfg(unix)]
#[test]
fn native_flash_folder_selection_keeps_files_when_an_entry_fails() {
    // Arrange: Put a valid file beside a symlink whose target does not exist.
    let directory = tempfile::tempdir().unwrap();
    let file_name = "1      ASCII   PROG [Readable] 1.txt";
    std::fs::write(directory.path().join(file_name), b"10 END").unwrap();
    let broken_link = directory.path().join("broken-link");
    std::os::unix::fs::symlink(directory.path().join("missing-target"), &broken_link).unwrap();

    // Act: Process every entry in the selected folder.
    let selection = read_flash_directory(directory.path());

    // Assert: The readable file remains imported and the failed entry is explicit.
    assert_eq!(
        selection
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>(),
        [file_name]
    );
    assert!(
        selection
            .errors
            .iter()
            .any(|error| error.contains(&broken_link.display().to_string()))
    );
}

// What: Route a real folder selection through the import control and display folder errors.
// Why: The UI must connect the folder reader to the existing partial-success and error notices.
#[test]
fn native_flash_folder_import_ui_lists_files_and_reports_folder_errors() {
    // Arrange: Create a selected folder containing one valid Flash Drive file.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    app.show_flash_directory = true;
    let now = app.last_update;
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("1      ASCII   PROG [Folder] 1.txt"),
        b"10 END",
    )
    .unwrap();
    let _ = app_frame(&mut app, &context, now, vec![]);
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, pos)| (text == "Import folder contents...").then_some(pos))
        .unwrap();
    let mut import = || Some(read_flash_directory(directory.path()));
    let mut clock = || now;

    // Act: Click Import and paint the selected folder's result.
    click_import(&mut app, &context, now, position, &mut import, &mut clock);
    let text = import_frame(&mut app, &context, now, vec![], &mut import, &mut clock);

    // Assert: The file appears in Flash Drive storage and in the rendered directory view.
    assert_eq!(app.machine.flash_drive_directory().len(), 1);
    assert!(text.iter().any(|text| text.contains("Folder")));

    // Act: Select a missing folder through the same deterministic picker seam.
    let missing = directory.path().join("missing");
    let mut import_missing = || Some(read_flash_directory(&missing));
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, pos)| (text == "Import folder contents...").then_some(pos))
        .unwrap();
    click_import(
        &mut app,
        &context,
        now,
        position,
        &mut import_missing,
        &mut clock,
    );
    let text = import_frame(
        &mut app,
        &context,
        now,
        vec![],
        &mut import_missing,
        &mut clock,
    );

    // Assert: The folder error is surfaced in both the notices and the rendered UI.
    assert!(
        app.import_notices
            .iter()
            .any(|(_, notice)| notice.contains(&missing.display().to_string()))
    );
    assert!(
        text.iter()
            .any(|notice| notice.contains(&missing.display().to_string()))
    );
}

// What: Read a Flash Drive ZIP bundle as named byte-preserving entries, ignoring directory records.
// Why: Bundled files must reuse numbered-file import semantics without extracting onto the host.
#[test]
fn native_flash_archive_reader_imports_entries_and_skips_directories() {
    // Arrange: Build a ZIP with a directory and two Flash Drive files in reverse number order.
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default();
    let compressed = options.compression_method(zip::CompressionMethod::Deflated);
    writer.add_directory("folder/", options).unwrap();
    writer
        .start_file("folder/2      BINARY  DATA [Data] 2.dat", compressed)
        .unwrap();
    writer.write_all(&[0, 0xff]).unwrap();
    writer
        .start_file("1      ASCII   PROG [Program] 9.txt", options)
        .unwrap();
    writer.write_all(b"10 END\r\r").unwrap();
    let archive_bytes = writer.finish().unwrap().into_inner();

    // Act: Decode the bundle in memory and import its entries through the public machine API.
    let selection = read_flash_archive_from(
        Cursor::new(archive_bytes),
        &mut FlashArchiveBudget::default(),
    )
    .unwrap();
    let mut machine = Machine::default();
    assert_eq!(
        machine.import_flash_files(&selection.files),
        [Ok(()), Ok(())]
    );

    // Assert: Directory records are omitted; both canonical files retain their exact bytes.
    assert!(selection.errors.is_empty());
    assert_eq!(machine.flash_drive_directory().len(), 2);
    assert_eq!(machine.export_flash_file(1).unwrap().bytes, b"10 END\r\r");
    assert_eq!(machine.export_flash_file(2).unwrap().bytes, [0, 0xff]);
}

// What: Reject malformed and empty ZIP input, and report a missing selected archive path.
// Why: A failed or empty archive must not appear as a successful storage import.
#[test]
fn native_flash_archive_reader_reports_invalid_empty_and_unreadable_archives() {
    // Arrange: Supply malformed bytes and a valid ZIP containing only a directory.
    let malformed_bytes = b"not a ZIP archive";
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory("empty/", SimpleFileOptions::default())
        .unwrap();
    let empty_bytes = writer.finish().unwrap().into_inner();

    // Act: Decode both in-memory inputs and attempt to open a missing selected archive.
    let malformed = read_flash_archive_from(
        Cursor::new(malformed_bytes),
        &mut FlashArchiveBudget::default(),
    );
    let empty =
        read_flash_archive_from(Cursor::new(empty_bytes), &mut FlashArchiveBudget::default());
    let directory = tempfile::tempdir().unwrap();
    let missing_path = directory.path().join("missing.zip");
    let unreadable = read_flash_archive(&missing_path, &mut FlashArchiveBudget::default());

    // Assert: Each unusable input produces an explicit error with the host path when available.
    assert!(
        malformed
            .err()
            .expect("malformed ZIP unexpectedly opened")
            .contains("invalid ZIP archive")
    );
    assert_eq!(
        empty.err().expect("directory-only ZIP unexpectedly opened"),
        "ZIP archive contains no files"
    );
    let unreadable = unreadable
        .err()
        .expect("missing ZIP file unexpectedly opened");
    assert!(unreadable.contains(&missing_path.display().to_string()));
    assert!(unreadable.starts_with("Import failed for "));
}

// What: Reject ZIP files larger than the compressed-input limit before parsing their directory.
// Why: Central-directory metadata must be size-bounded before the ZIP reader materializes it.
#[test]
fn native_flash_archive_reader_bounds_compressed_file_size_before_parsing() {
    // Arrange: Create a sparse selected file one byte larger than the accepted ZIP size.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("oversized.zip");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_FLASH_ARCHIVE_FILE_BYTES + 1)
        .unwrap();
    let mut budget = FlashArchiveBudget::default();

    // Act: Open the selected archive through the file-backed bounded reader.
    let error = read_flash_archive(&path, &mut budget)
        .err()
        .expect("oversized ZIP unexpectedly reached the parser");

    // Assert: The file-size guard reports the limit before parsing or spending batch budgets.
    assert!(error.contains("128 MiB compressed file limit"));
    assert_eq!(budget.remaining_entries, MAX_FLASH_ARCHIVE_ENTRIES);
    assert_eq!(budget.remaining_bytes, MAX_FLASH_ARCHIVE_TOTAL_BYTES);
}

// What: Allow an archive at the compressed-input limit to reach ZIP parsing.
// Why: The configured file-size cap is inclusive, so its exact boundary must not be rejected.
#[test]
fn native_flash_archive_reader_allows_exact_compressed_file_limit_to_reach_parser() {
    // Arrange: Create a sparse selected file whose size exactly matches the accepted limit.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("at-limit.zip");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_FLASH_ARCHIVE_FILE_BYTES)
        .unwrap();
    let mut budget = FlashArchiveBudget::default();

    // Act: Open the zero-filled file; it is invalid ZIP data but passes the size guard.
    let error = read_flash_archive(&path, &mut budget)
        .err()
        .expect("invalid ZIP unexpectedly opened");

    // Assert: The ZIP parser reports invalid contents rather than the compressed-size limit.
    assert!(error.contains("invalid ZIP archive"), "{error}");
    assert!(!error.contains("128 MiB compressed file limit"), "{error}");
    assert_eq!(budget.remaining_entries, MAX_FLASH_ARCHIVE_ENTRIES);
    assert_eq!(budget.remaining_bytes, MAX_FLASH_ARCHIVE_TOTAL_BYTES);
}

// What: Reject ZIP entries whose uncompressed contents exceed the per-file limit.
// Why: Compressed input must not allocate unbounded memory in the native emulator.
#[test]
fn native_flash_archive_reader_bounds_entry_expansion() {
    // Arrange: Compress one file exactly at the entry limit and another one byte over it.
    let payload = vec![0; MAX_FLASH_ARCHIVE_ENTRY_BYTES as usize + 1];
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("1      BINARY  DATA [Boundary] 0", options)
        .unwrap();
    writer
        .write_all(&payload[..MAX_FLASH_ARCHIVE_ENTRY_BYTES as usize])
        .unwrap();
    writer
        .start_file("2      BINARY  DATA [Large] 0", options)
        .unwrap();
    writer.write_all(&payload).unwrap();
    let archive_bytes = writer.finish().unwrap().into_inner();

    // Act: Read the archive with bounded decompression.
    let selection = read_flash_archive_from(
        Cursor::new(archive_bytes),
        &mut FlashArchiveBudget::default(),
    )
    .unwrap();

    // Assert: The boundary-sized file succeeds and the oversized sibling is reported.
    assert_eq!(selection.files.len(), 1);
    assert_eq!(
        selection.files[0].bytes.len(),
        MAX_FLASH_ARCHIVE_ENTRY_BYTES as usize
    );
    assert_eq!(selection.errors.len(), 1);
    assert!(selection.errors[0].contains("8 MiB uncompressed limit"));
}

fn create_flash_test_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

// What: Share byte and entry budgets across multiple ZIPs in one file selection.
// Why: Selecting several individually valid archives must not multiply the chosen resource caps.
#[test]
fn native_flash_zip_selection_shares_total_and_entry_budgets() {
    // Arrange: Prepare three small valid bundles and deliberately tight test budgets.
    let directory = tempfile::tempdir().unwrap();
    let paths = [
        (
            "first.zip",
            "1      ASCII   PROG [First] 3",
            b"one".as_slice(),
        ),
        (
            "second.zip",
            "2      ASCII   PROG [Second] 1",
            b"2".as_slice(),
        ),
        (
            "third.zip",
            "3      ASCII   PROG [Third] 1",
            b"3".as_slice(),
        ),
    ]
    .map(|(filename, entry, bytes)| {
        let path = directory.path().join(filename);
        std::fs::write(&path, create_flash_test_archive(&[(entry, bytes)])).unwrap();
        path
    });

    // Act: Import all archives once with a four-byte budget and once with a two-entry budget.
    let mut byte_budget = FlashArchiveBudget {
        remaining_entries: 10,
        remaining_bytes: 4,
    };
    let byte_selection = read_flash_paths_with_budget(paths.clone(), &mut byte_budget);
    let mut entry_budget = FlashArchiveBudget {
        remaining_entries: 2,
        remaining_bytes: 100,
    };
    let entry_selection = read_flash_paths_with_budget(paths, &mut entry_budget);

    // Assert: Both batches keep their within-limit files and explicitly reject the next ZIP.
    assert_eq!(byte_selection.files.len(), 2);
    assert_eq!(byte_selection.files[0].bytes, b"one");
    assert_eq!(byte_selection.files[1].bytes, b"2");
    assert_eq!(byte_selection.errors.len(), 1);
    assert!(byte_selection.errors[0].contains("total uncompressed limit"));
    assert_eq!(entry_selection.files.len(), 2);
    assert_eq!(entry_selection.errors.len(), 1);
    assert!(entry_selection.errors[0].contains("only 0 remain"));
}

// What: Accept exactly the selected-import entry limit and reject one additional archive member.
// Why: The cap must be inclusive and apply before processing an oversized ZIP's entries.
#[test]
fn native_flash_archive_reader_enforces_entry_count_boundary() {
    // Arrange: Build otherwise valid empty ZIP files at and one above the default entry limit.
    let make_archive = |count: usize| {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..count {
            writer
                .start_file(format!("entry-{index}.dat"), SimpleFileOptions::default())
                .unwrap();
        }
        writer.finish().unwrap().into_inner()
    };
    let at_limit = make_archive(MAX_FLASH_ARCHIVE_ENTRIES);
    let over_limit = make_archive(MAX_FLASH_ARCHIVE_ENTRIES + 1);

    // Act: Decode each archive using an independent default selected-import budget.
    let accepted =
        read_flash_archive_from(Cursor::new(at_limit), &mut FlashArchiveBudget::default()).unwrap();
    let rejected =
        read_flash_archive_from(Cursor::new(over_limit), &mut FlashArchiveBudget::default());

    // Assert: The limit is inclusive and the next archive is rejected with a clear error.
    assert_eq!(accepted.files.len(), MAX_FLASH_ARCHIVE_ENTRIES);
    assert!(
        rejected
            .err()
            .expect("over-limit archive unexpectedly opened")
            .contains("4096 remain")
    );
}

// What: Keep valid ZIP entries when another member fails decompression or CRC validation.
// Why: A damaged archive member must be reported without discarding independent files.
#[test]
fn native_flash_archive_reader_reports_corrupt_members_and_keeps_good_files() {
    // Arrange: Create a stored file whose payload can be corrupted without changing the directory.
    let corrupt_payload = b"corrupt member contents";
    let good_payload = b"good file";
    let mut archive_bytes = create_flash_test_archive(&[
        ("1      BINARY  DATA [Corrupt] 23", corrupt_payload),
        ("2      BINARY  DATA [Good] 9", good_payload),
    ]);
    let payload_position = archive_bytes
        .windows(corrupt_payload.len())
        .position(|window| window == corrupt_payload)
        .unwrap();
    archive_bytes[payload_position] ^= 0xff;

    // Act: Read both members through the normal bounded ZIP parser.
    let selection = read_flash_archive_from(
        Cursor::new(archive_bytes),
        &mut FlashArchiveBudget::default(),
    )
    .unwrap();

    // Assert: The good member survives and the corrupted member's name appears in the error.
    assert_eq!(selection.files.len(), 1);
    assert_eq!(selection.files[0].name, "2      BINARY  DATA [Good] 9");
    assert_eq!(selection.files[0].bytes, good_payload);
    assert_eq!(selection.errors.len(), 1);
    assert!(selection.errors[0].contains("1      BINARY  DATA [Corrupt] 23"));
}

// What: Route a ZIP path through the native import control, including folder entries and a bad filename.
// Why: Archive detection, basename import and partial-error notices must work together at the UI boundary.
#[test]
fn flash_zip_import_dispatches_entries_and_reports_partial_failure() {
    // Arrange: Create a ZIP selected through an uppercase extension with nested valid and invalid files.
    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("drive.ZIP");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer
        .start_file("folder/1      ASCII   PROG [Nested] 9.txt", options)
        .unwrap();
    writer.write_all(b"10 END\r\r").unwrap();
    writer
        .start_file("folder/not a Flash Drive file", options)
        .unwrap();
    writer.write_all(b"invalid").unwrap();
    std::fs::write(&archive_path, writer.finish().unwrap().into_inner()).unwrap();

    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let _ = app_frame(&mut app, &context, now, vec![]);
    let button = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, position)| (text == "Import folder contents...").then_some(position))
        .unwrap();
    let mut selection = Some(read_flash_paths([archive_path]));
    let mut import = || selection.take();
    let mut clock = || now;

    // Act: Click Import and let the normal UI path display its notices.
    click_import(&mut app, &context, now, button, &mut import, &mut clock);
    let text = import_frame(&mut app, &context, now, vec![], &mut import, &mut clock);

    // Assert: The nested file imports by basename and its invalid sibling appears as a failure.
    assert!(
        text.iter()
            .any(|text| text == "1 Flash Drive file(s) imported")
    );
    assert!(
        text.iter()
            .any(|text| text.contains("not a Flash Drive file"))
    );
    assert_eq!(
        app.machine.export_flash_file(1).unwrap().bytes,
        b"10 END\r\r"
    );
    assert_eq!(
        app.import_notices
            .iter()
            .filter(|(is_error, _)| *is_error)
            .count(),
        1
    );
}

// What: Inspect the display uploads at creation and after a framebuffer revision.
// Why: Smooth presentation must survive updates without changing the emulated pixels.
#[test]
fn display_uploads_keep_linear_filtering_and_original_pixels() {
    // Arrange: Draw a stored vector and a dim cursor at distinct coordinates.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
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
    machine.write_byte(0x8794, 0).unwrap();
    machine.write_byte(0x8796, 12).unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let texture_id = app.display_texture.id();

    for extend in [false, true] {
        // Act: Collect the initial upload, then extend the dim cursor for a new revision.
        if extend {
            app.machine.write_byte(0x8796, 20).unwrap();
        }
        let output = context.run(RawInput::default(), |_| app.update_display_texture());
        let upload = output
            .textures_delta
            .set
            .iter()
            .find_map(|(id, delta)| (*id == texture_id).then_some(delta))
            .expect("missing display upload");

        // Assert: Both uploads request interpolation and retain every source intensity.
        assert_eq!(upload.options.magnification, egui::TextureFilter::Linear);
        assert_eq!(upload.options.minification, egui::TextureFilter::Linear);
        assert_eq!(
            upload.image.size(),
            [CpuReferenceRenderer::WIDTH, CpuReferenceRenderer::HEIGHT]
        );
        let egui::ImageData::Color(image) = &upload.image;
        assert_eq!(image.pixels.len(), app.display().pixels().len());
        assert!(
            image
                .pixels
                .iter()
                .zip(app.display().pixels())
                .all(|(pixel, &intensity)| *pixel == egui::Color32::from_rgb(0, intensity, 0))
        );
    }
}

fn app_frame(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
        events,
        ..Default::default()
    };
    app_frame_with_input(app, context, now, input)
}

fn advance_until_prompt(app: &mut Tek4051App, context: &egui::Context, start: Instant) -> Instant {
    let mut now = start;
    for _ in 0..3_000 {
        if app.basic_prompt_ready {
            return now;
        }
        now += Duration::from_millis(10);
        app_frame(app, context, now, vec![]);
    }
    panic!("the ROM did not draw its BASIC cursor within the test safety bound");
}

fn app_frame_at_egui_time(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    time: f64,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
        time: Some(time),
        events,
        ..Default::default()
    };
    app_frame_with_input(app, context, now, input)
}

fn app_frame_with_input(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    input: RawInput,
) -> Vec<(String, Pos2)> {
    context
        .run(input, |context| app.update_at(context, now))
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some((text.galley.job.text.clone(), text.pos + Vec2::splat(2.0))),
            _ => None,
        })
        .collect()
}

fn indicator_colors(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
) -> Vec<egui::Color32> {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
        ..Default::default()
    };
    let shapes = context
        .run(input, |context| app.update_at(context, now))
        .shapes;
    ["BUSY", "BREAK", "I/O", "POWER"]
        .iter()
        .map(|label| {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.job.text == *label => {
                        Some(text.galley.job.sections[0].format.color)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| {
                    panic!(
                        "missing {label}; rendered text: {:?}",
                        shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                Shape::Text(text) => Some(text.galley.job.text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    )
                })
        })
        .collect()
}

fn click_app_control(app: &mut Tek4051App, context: &egui::Context, now: Instant, label: &str) {
    let position = app_frame(app, context, now, vec![])
        .into_iter()
        .find_map(|(text, position)| (text == label).then_some(position))
        .unwrap();
    for pressed in [true, false] {
        app_frame(
            app,
            context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

fn directory_number_position(shapes: &[(String, Pos2)], number: u8) -> Pos2 {
    let number_heading_y = shapes
        .iter()
        .find_map(|(text, position)| (text == "Number").then_some(position.y))
        .unwrap();
    shapes
        .iter()
        .find_map(|(text, position)| {
            (text == &number.to_string() && position.y > number_heading_y).then_some(*position)
        })
        .unwrap()
}

fn click_app_position(app: &mut Tek4051App, context: &egui::Context, now: Instant, pos: Pos2) {
    for pressed in [true, false] {
        app_frame(
            app,
            context,
            now,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

fn click_app_position_at_time(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    pos: Pos2,
    time: f64,
) {
    for (pressed, frame_time) in [(true, time), (false, time + 0.01)] {
        app_frame_at_egui_time(
            app,
            context,
            now,
            frame_time,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

fn double_click_app_position_at_time(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    pos: Pos2,
    time: f64,
) -> Vec<(String, Pos2)> {
    click_app_position_at_time(app, context, now, pos, time);
    click_app_position_at_time(app, context, now, pos, time + 0.1);
    app_frame_at_egui_time(app, context, now, time + 0.2, vec![])
}

fn app_key_event(key: egui::Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn send_app_key(app: &mut Tek4051App, context: &egui::Context, now: Instant, key: egui::Key) {
    app_frame(app, context, now, vec![app_key_event(key, true)]);
    app_frame(app, context, now, vec![app_key_event(key, false)]);
}

fn send_directory_arrow(
    app: &mut Tek4051App,
    context: &egui::Context,
    now: Instant,
    key: egui::Key,
) {
    assert!(matches!(key, egui::Key::ArrowUp | egui::Key::ArrowDown));
    app_frame(app, context, now, vec![app_key_event(key, true)]);
    assert!(
        app.keyboard.is_idle(),
        "directory navigation key reached the emulator keyboard"
    );
    app_frame(app, context, now, vec![app_key_event(key, false)]);
    assert!(app.keyboard.is_idle());
}

fn speed_control_frame(
    context: &egui::Context,
    runtime: &mut HostRuntime,
    speed_index: &mut usize,
    settings_error: &mut Option<String>,
    events: Vec<Event>,
) -> (Rect, Vec<String>) {
    let mut button_rect = Rect::NOTHING;
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 100.0))),
        events,
        ..Default::default()
    };
    let output = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            button_rect = speed_control(ui, runtime, speed_index, settings_error).rect;
        });
    });
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    (button_rect, text)
}

fn click_speed_control(
    context: &egui::Context,
    runtime: &mut HostRuntime,
    speed_index: &mut usize,
    settings_error: &mut Option<String>,
    position: Pos2,
) {
    for pressed in [true, false] {
        speed_control_frame(
            context,
            runtime,
            speed_index,
            settings_error,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

// What: Click through all reference speed settings and execute at each selected speed.
// Why: Every displayed setting must apply its corresponding CPU budget and wrap back to x1.
#[test]
fn speed_button_updates_runtime_and_wraps_after_x100() {
    // Arrange: A long NOP loop yields a different PC for every reference instruction budget.
    let context = egui::Context::default();
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let start = 0x0100;
    for offset in 0..30_000 {
        machine.write_byte(start + offset, 0x01).unwrap();
    }

    machine.write_byte(start + 30_000, 0x7e).unwrap();
    machine.write_byte(start + 30_001, 0x01).unwrap();
    machine.write_byte(start + 30_002, 0x00).unwrap();
    machine.cpu_mut().registers.pc = start;
    let mut runtime = HostRuntime::start(&mut machine);
    let mut speed_index = 0;
    let mut settings_error = None;
    let (button, initial_text) = speed_control_frame(
        &context,
        &mut runtime,
        &mut speed_index,
        &mut settings_error,
        vec![],
    );
    assert_eq!(initial_text, ["x1"]);

    // Act: Select each speed, repaint its label, and run one 10 ms CPU interval.
    for (expected_label, expected_pc_offset) in [
        ("x2", 4_528),
        ("x5", 11_318),
        ("x10", 22_636),
        ("x20", 15_271),
        ("x50", 23_177),
        ("x100", 16_352),
        ("x1", 2_264),
    ] {
        click_speed_control(
            &context,
            &mut runtime,
            &mut speed_index,
            &mut settings_error,
            button.center(),
        );
        let (_, text) = speed_control_frame(
            &context,
            &mut runtime,
            &mut speed_index,
            &mut settings_error,
            vec![],
        );
        machine.cpu_mut().registers.pc = start;
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();

        // Assert: The visible label and executed budget match this reference speed.
        assert_eq!(text, [expected_label]);
        assert_eq!(machine.registers().pc, start + expected_pc_offset);
        assert_eq!(settings_error, None);
    }
}

// What: Change speed after an elapsed interval in the native application update.
// Why: A new speed must not retroactively change the prior interval's instruction budget.
#[test]
fn speed_click_applies_after_previous_elapsed_interval() {
    // Arrange: Start the app on enough NOPs to observe x1 followed by x2.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for offset in 0..7_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }

    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    let initial_text = app_frame(&mut app, &context, start, vec![]);
    assert!(initial_text.iter().any(|(text, _)| text == "Start"));
    click_app_control(&mut app, &context, start, "Start");
    let text = app_frame(&mut app, &context, start, vec![]);
    let position = text.iter().find(|(text, _)| text == "x1").unwrap().1;

    // Act: Open the selector, then choose x2 at the 10 ms boundary.
    for pressed in [true, false] {
        app_frame(
            &mut app,
            &context,
            start,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let options = app_frame(&mut app, &context, start, vec![]);
    let position = options.iter().find(|(text, _)| text == "x2").unwrap().1;
    for pressed in [true, false] {
        app_frame(
            &mut app,
            &context,
            start + Duration::from_millis(10),
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }

    // Assert: The prior interval used x1; the next interval uses the selected x2.
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264);
    app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(20),
        vec![],
    );
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264 + 4_528);
}

// What: Start and pause the native app around elapsed CPU callbacks.
// Why: The controls must freeze callbacks while paused and resume from a fresh interval.
#[test]
fn app_start_pause_controls_runtime_and_button_state() {
    // Arrange: Start on RAM NOPs with the application initially paused.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for offset in 0..5_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    let initial_text = app_frame(&mut app, &context, start, vec![]);

    // Act: Start, let one interval run, pause through a long interval, then start again.
    assert!(initial_text.iter().any(|(text, _)| text == "Start"));
    click_app_control(&mut app, &context, start, "Start");
    let before_first_deadline =
        app_frame(&mut app, &context, start + Duration::from_millis(9), vec![]);
    let pc_before_first_deadline = app.machine.registers().pc;
    app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(10),
        vec![],
    );
    let pc_after_first_deadline = app.machine.registers().pc;
    click_app_control(
        &mut app,
        &context,
        start + Duration::from_millis(10),
        "Pause",
    );
    let paused_frame = app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(110),
        vec![],
    );
    let pc_after_paused_interval = app.machine.registers().pc;
    click_app_control(
        &mut app,
        &context,
        start + Duration::from_millis(110),
        "Start",
    );
    let resumed_frame = app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(120),
        vec![],
    );

    // Assert: Labels track each state and only the two running deadlines advance the CPU.
    assert!(
        before_first_deadline
            .iter()
            .any(|(text, _)| text == "Pause")
    );
    assert_eq!(pc_before_first_deadline, 0x0100);
    assert_eq!(pc_after_first_deadline, 0x0100 + 2_264);
    assert!(paused_frame.iter().any(|(text, _)| text == "Start"));
    assert_eq!(pc_after_paused_interval, pc_after_first_deadline);
    assert!(resumed_frame.iter().any(|(text, _)| text == "Pause"));
    assert_eq!(app.machine.registers().pc, 0x0100 + 2 * 2_264);
}

// What: Trigger a machine failure during Start and try the disabled Start control again.
// Why: A stopped runtime must remain visibly stopped and cannot be resumed without reset.
#[test]
fn app_disables_start_after_runtime_failure() {
    // Arrange: Select an unsupported ROM bank so the first running callback fails.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.write_byte(0x87c0, 0x08).unwrap();
    machine.cpu_mut().registers.pc = 0x9800;
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;

    // Act: Start and reach the first CPU callback that reads the unsupported bank.
    click_app_control(&mut app, &context, start, "Start");
    let failure_frame = app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(10),
        vec![],
    );
    let runtime_error = app.runtime_error.clone().unwrap();
    click_app_control(
        &mut app,
        &context,
        start + Duration::from_millis(10),
        "Start",
    );
    let after_attempt = app_frame(
        &mut app,
        &context,
        start + Duration::from_millis(20),
        vec![],
    );

    // Assert: The error stays visible and the disabled control cannot resume execution.
    assert!(
        failure_frame
            .iter()
            .any(|(text, _)| text.starts_with("Emulation stopped: "))
    );
    assert!(after_attempt.iter().any(|(text, _)| text == "Start"));
    assert!(!app.running);
    assert_eq!(app.runtime_error.as_deref(), Some(runtime_error.as_str()));

    // Act: Reset the failed machine and render the recovered, paused app.
    let reset_at = start + Duration::from_millis(20);
    click_app_control(&mut app, &context, reset_at, "Reset");
    let reset_frame = app_frame(&mut app, &context, reset_at, vec![]);

    // Assert: Reset clears the failure, returns to the ROM vector, and starts the erase flash.
    assert!(reset_frame.iter().any(|(text, _)| text == "Start"));
    assert!(
        !reset_frame
            .iter()
            .any(|(text, _)| text.starts_with("Emulation stopped: "))
    );
    assert_eq!(app.machine.registers().pc, 0xbc4b);
    assert!(!app.running);
    assert!(app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(u8::MAX));
    let reset_pc = app.machine.registers().pc;

    // Act: Let the wall-clock reset erase deadline elapse while the app stays paused.
    app_frame(&mut app, &context, reset_at + DISPLAY_ERASE_DELAY, vec![]);

    // Assert: The paused display becomes ready and dark without starting CPU execution.
    assert!(!app.running);
    assert_eq!(app.machine.registers().pc, reset_pc);
    assert!(!app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(0));
}

// What: Complete a program-triggered erase while the native app displays an execution error.
// Why: The stopped UI must keep servicing independent erase timeouts, not only Reset erases.
#[test]
fn app_completes_program_erase_while_displaying_runtime_error() {
    // Arrange: Queue an erase and a short panel tap before a machine's failing CPU callback.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.erase_display();
    machine.write_byte(0x87c0, 0x08).unwrap();
    machine.cpu_mut().registers.pc = 0x9800;
    let mut app = Tek4051App::new(&creation, machine);
    let start = app.last_update;
    click_app_control(&mut app, &context, start, "Start");
    click_app_control(&mut app, &context, start, "HOME / PAGE");

    // Act: Fail at 10 ms within a 499 ms frame split by the panel's 100 ms tap interval.
    app_frame(
        &mut app,
        &context,
        start + DISPLAY_ERASE_DELAY - Duration::from_millis(1),
        vec![],
    );
    let error = app.runtime_error.clone().expect("expected execution error");
    let stopped_pc = app.machine.registers().pc;

    // Assert: The error remains visible and the flash is still pending before 500 ms.
    assert_eq!(app.runtime_error.as_deref(), Some(error.as_str()));
    assert!(app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(255));

    // Act: Render at the independent erase deadline without resetting or resuming.
    let completed = app_frame(&mut app, &context, start + DISPLAY_ERASE_DELAY, vec![]);

    // Assert: The flash clears and readiness returns while execution remains stopped.
    assert!(
        completed
            .iter()
            .any(|(text, _)| text.starts_with("Emulation stopped: "))
    );
    assert_eq!(app.runtime_error.as_deref(), Some(error.as_str()));
    assert!(!app.running);
    assert_eq!(app.machine.registers().pc, stopped_pc);
    assert!(!app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(0));
}

// What: Reset the application while its runtime is active.
// Why: Reset must preserve the user's running state while restarting the machine.
#[test]
fn app_reset_preserves_running_state() {
    // Arrange: Start the app from its normal reset vector and capture its frame time.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    click_app_control(&mut app, &context, now, "Start");
    assert!(app.running);

    // Act: Reset without pausing the running host runtime.
    click_app_control(&mut app, &context, now, "Reset");
    let reset_frame = app_frame(&mut app, &context, now, vec![]);

    // Assert: The runtime remains active and the reference erase flash is visible.
    assert!(reset_frame.iter().any(|(text, _)| text == "Pause"));
    assert!(app.running);
    assert_eq!(app.runtime_error, None);
    assert!(app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(u8::MAX));
    let reset_pc = app.machine.registers().pc;

    // Act: Run one callback, then advance to just before the reset erase deadline.
    app_frame(&mut app, &context, now + Duration::from_millis(10), vec![]);
    assert_ne!(app.machine.registers().pc, reset_pc);
    let before_deadline = app_frame(
        &mut app,
        &context,
        now + DISPLAY_ERASE_DELAY - Duration::from_millis(1),
        vec![],
    );
    assert!(app.machine.display_erase_pending());
    let completed = app_frame(&mut app, &context, now + DISPLAY_ERASE_DELAY, vec![]);

    // Assert: The app's live runtime reaches the erase boundary without losing running state.
    assert!(before_deadline.iter().any(|(text, _)| text == "Pause"));
    assert!(completed.iter().any(|(text, _)| text == "Pause"));
    assert!(app.running);
    assert!(!app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(0));
}

// What: Paint indicator states before and after keyboard output writes and run controls.
// Why: The user must see independent PIA lights and the active or paused POWER state.
#[test]
fn app_paints_status_indicators_from_machine_and_run_state() {
    // Arrange: Start with the app paused and keyboard PIA output selected.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let dim = egui::Color32::from_rgb(0, 85, 0);
    let bright = egui::Color32::from_rgb(0, 238, 0);
    app_frame(&mut app, &context, now, vec![]);

    // Act: Inspect initial lights, set BUSY and I/O, then start and pause the app.
    let initial = indicator_colors(&mut app, &context, now);
    app.machine.write_byte(0x87aa, 0xc0).unwrap();
    let paused = indicator_colors(&mut app, &context, now);
    click_app_control(&mut app, &context, now, "Start");
    let running = indicator_colors(&mut app, &context, now);
    click_app_control(&mut app, &context, now, "Pause");
    app.machine.write_byte(0x87aa, 0x20).unwrap();
    let after_pause = indicator_colors(&mut app, &context, now);

    // Assert: Machine-driven colors follow output bits and POWER follows execution state.
    assert_eq!(initial, [dim, dim, dim, dim]);
    assert_eq!(paused, [bright, dim, bright, dim]);
    assert_eq!(running, [bright, dim, bright, bright]);
    assert_eq!(after_pause, [dim, bright, dim, dim]);
}

// What: Toggle Caps Lock from the native UI and sample the keyboard PIA input.
// Why: The visible control must change the line consumed by the base ROM.
#[test]
fn caps_lock_checkbox_toggles_the_keyboard_tty_line() {
    // Arrange: Select keyboard data registers and locate the rendered checkbox label.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let position = app_frame(&mut app, &context, now, vec![])
        .into_iter()
        .find_map(|(text, position)| (text == "Caps Lock").then_some(position))
        .unwrap();

    // Act: Turn the control on, sample the ROM-facing bit, then turn it off.
    for pressed in [true, false] {
        app_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    app.machine.keyboard_press(b'B');
    let caps_lock_on = app.machine.read_byte(0x87a8).unwrap();
    app.machine.keyboard_release();
    for pressed in [true, false] {
        app_frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    app.machine.keyboard_press(b'B');
    let caps_lock_off = app.machine.read_byte(0x87a8).unwrap();

    // Assert: Each checkbox transition flips the Caps Lock bit in the key byte.
    assert_eq!(caps_lock_on, 0x42);
    assert_eq!(caps_lock_off, 0xc2);
}

// What: Encode a full display snapshot and report an inaccessible destination.
// Why: Screen copies must preserve green intensities and surface file failures.
#[test]
fn screen_copy_png_preserves_pixels_and_reports_file_errors() {
    // Arrange: Prepare distinct pixels at both ends of a full-size snapshot.
    let mut pixels = vec![0; CpuReferenceRenderer::WIDTH * CpuReferenceRenderer::HEIGHT];
    pixels[0] = 200;
    *pixels.last_mut().unwrap() = 254;

    // Act: Save and decode a PNG, then try writing to a path that is a directory.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("screen.png");
    save_screen(&path, &pixels).unwrap();
    let decoded = image::open(&path).unwrap().to_rgba8();
    let error = save_screen(Path::new("."), &pixels).unwrap_err();

    // Assert: PNG dimensions and RGBA pixels match; destination errors are returned.
    assert_eq!(
        decoded.dimensions(),
        (
            CpuReferenceRenderer::WIDTH as u32,
            CpuReferenceRenderer::HEIGHT as u32
        )
    );
    assert_eq!(decoded.get_pixel(0, 0).0, [0, 200, 0, 255]);
    assert_eq!(
        decoded
            .get_pixel(
                CpuReferenceRenderer::WIDTH as u32 - 1,
                CpuReferenceRenderer::HEIGHT as u32 - 1
            )
            .0,
        [0, 254, 0, 255]
    );
    assert!(matches!(error, image::ImageError::IoError(_)));
}

// What: Request a machine copy and click Make Copy in the full app.
// Why: Both sources must reach the save adapter and report outcomes to the user.
#[test]
fn screen_copy_sources_reach_save_and_show_results() {
    // Arrange: Use a fake save adapter so no native dialog opens during the test.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    let now = app.last_update;
    let mut clock = || now;
    let mut snapshots = Vec::new();
    let mut save = |pixels: &[u8]| {
        snapshots.push(pixels.to_vec());
        match snapshots.len() {
            1 => Ok(Some(PathBuf::from("first.png"))),
            2 => Err(io::Error::other("save failed").to_string()),
            _ => Ok(None),
        }
    };
    app.machine.write_byte(0x8795, 0x04).unwrap();
    app.machine.write_byte(0x8794, 0x20).unwrap();
    app.machine.write_byte(0x8794, 0).unwrap();
    let input = |events| RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
        events,
        ..Default::default()
    };

    // Act: Process the machine request, then click the painted Make Copy button.
    let _ = context.run(input(vec![]), |context| {
        app.update_at_with_dialogs(
            context,
            now,
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });
    let output = context.run(input(vec![]), |context| {
        app.update_at_with_dialogs(
            context,
            now,
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::Text(text) if text.galley.job.text == "MAKE COPY" => {
                Some(text.pos + Vec2::splat(2.0))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(app.screen_copy_notices[0].1, "Screen copy saved: first.png");
    for pressed in [true, false] {
        let _ = context.run(
            input(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]),
            |context| {
                app.update_at_with_dialogs(
                    context,
                    now,
                    &mut save,
                    &mut || None,
                    &mut |_| Ok(None),
                    &mut clock,
                )
            },
        );
    }

    // Assert: The failed save is visible.
    assert_eq!(
        app.screen_copy_notices,
        [(true, "Screen copy failed: save failed".to_owned())]
    );

    // Act: Click Make Copy again and cancel the save dialog.
    for pressed in [true, false] {
        let _ = context.run(
            input(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]),
            |context| {
                app.update_at_with_dialogs(
                    context,
                    now,
                    &mut save,
                    &mut || None,
                    &mut |_| Ok(None),
                    &mut clock,
                )
            },
        );
    }

    // Assert: Cancellation is reported and every request delivered the same full image.
    assert_eq!(
        app.screen_copy_notices,
        [(false, "Screen copy cancelled".to_owned())]
    );
    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        snapshots[0].len(),
        CpuReferenceRenderer::WIDTH * CpuReferenceRenderer::HEIGHT
    );
    assert_eq!(snapshots[0], snapshots[1]);
    assert_eq!(snapshots[1], snapshots[2]);
}

// What: Save a copy triggered between two display updates in the running app.
// Why: The exported PNG must preserve the machine's event-time pixel coordinates and intensity.
#[test]
fn machine_copy_saves_event_time_display_pixels() {
    // Arrange: Draw a stored vector and trigger a copy before extending it.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    for (port, control, value) in [
        (0x878c, 0x878d, 0),
        (0x878e, 0x878f, 20),
        (0x8794, 0x8795, 0x80),
        (0x8796, 0x8797, 5),
    ] {
        app.machine.write_byte(port, 0xff).unwrap();
        app.machine.write_byte(control, 0x04).unwrap();
        app.machine.write_byte(port, value).unwrap();
    }
    app.machine.write_byte(0x8796, 10).unwrap();
    app.machine.write_byte(0x8794, 0xa0).unwrap();
    app.machine.write_byte(0x8794, 0x80).unwrap();
    app.machine.write_byte(0x8796, 20).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("screen.png");
    let mut save = |pixels: &[u8]| {
        save_screen(&path, pixels).map_err(|error| error.to_string())?;
        Ok(Some(path.clone()))
    };
    let now = app.last_update;
    let mut clock = || now;

    // Act: Process the pending machine copy through the app and decode its PNG.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_dialogs(
            context,
            now,
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });
    let image = image::open(&path).unwrap().to_rgba8();

    // Assert: The earlier asymmetric vector is present, but the later extension is absent.
    assert_eq!(image.get_pixel(26, 799).0, [0, 240, 0, 255]);
    assert_eq!(image.get_pixel(36, 799).0, [0, 0, 0, 255]);
    assert_eq!(app.display().pixel_intensity(36, 799), Some(240));
}

// What: Advance the app after a simulated five-second save dialog with an erase pending.
// Why: Dialog time must expire erase timeouts without being added to the CPU interval.
#[test]
fn screen_copy_dialog_time_does_not_advance_the_machine() {
    // Arrange: Use NOPs to observe one 10 ms budget and request erase before copying the flash.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    for offset in 0..5_000 {
        machine.write_byte(0x0100 + offset, 0x01).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    let mut app = Tek4051App::new(&creation, machine);
    let app_start = app.last_update;
    click_app_control(&mut app, &context, app_start, "Start");
    app.machine.erase_display();
    app.machine.write_byte(0x8795, 0x04).unwrap();
    app.machine.write_byte(0x8794, 0x20).unwrap();
    app.machine.write_byte(0x8794, 0).unwrap();
    let start = app.last_update;
    let after_dialog = start + Duration::from_secs(5);
    let mut clock = || after_dialog;
    let mut save = |_: &[u8]| Ok(None);

    // Act: Process the copy, then advance 10 ms after the dialog closes.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_dialogs(
            context,
            start,
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });

    // Assert: Closing the dialog completes the erase without executing CPU instructions.
    assert!(!app.machine.display_erase_pending());
    assert_eq!(app.display().pixel_intensity(0, 0), Some(0));
    assert_eq!(app.machine.registers().pc, 0x0100);

    // Act: Advance the first interval after the dialog closes.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_dialogs(
            context,
            after_dialog + Duration::from_millis(10),
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });

    // Assert: The CPU ran only the single x1 budget, not five seconds of callbacks.
    assert_eq!(app.machine.registers().pc, 0x0100 + 2_264);
    assert!(app.runtime_error.is_none());
}

// What: Process more machine copy requests than the app can buffer.
// Why: Lost requests must be visible to the user without halting emulation.
#[test]
fn screen_copy_overflow_is_reported_without_stopping_runtime() {
    // Arrange: Queue eight snapshots and overflow twice.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app = Tek4051App::new(&creation, machine);
    app.machine.write_byte(0x8795, 0x04).unwrap();
    for _ in 0..10 {
        app.machine.write_byte(0x8794, 0x20).unwrap();
        app.machine.write_byte(0x8794, 0).unwrap();
    }
    let now = app.last_update;
    let mut clock = || now;
    let mut saves = 0;
    let mut save = |_: &[u8]| {
        saves += 1;
        Ok(None)
    };

    // Act: Process pending requests and advance the next emulation interval.
    let _ = context.run(RawInput::default(), |context| {
        app.update_at_with_dialogs(
            context,
            now,
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });
    let frame = context.run(RawInput::default(), |context| {
        app.update_at_with_dialogs(
            context,
            now + Duration::from_millis(10),
            &mut save,
            &mut || None,
            &mut |_| Ok(None),
            &mut clock,
        );
    });

    // Assert: Both losses are reported, buffered copies are handled, and the runtime is healthy.
    assert_eq!(saves, 8);
    assert!(
        app.screen_copy_notices
            .iter()
            .any(|(error, message)| *error && message.contains("At least 2"))
    );
    assert!(frame.shapes.iter().any(|shape| matches!(
        &shape.shape,
        Shape::Text(text)
            if text.galley.job.text == "At least 2 screen copy request(s) could not be saved: queue full"
    )));
    assert!(app.runtime_error.is_none());
}
