use std::{
    io::{Cursor, Read, Write},
    time::{Duration, Instant},
};

use super::{BrowserArchiveWriter, BrowserAutoLoadState, BrowserEmulator, map_browser_key};
use crate::emulation::CpuReferenceRenderer;
use crate::machine::gpib::FlashDriveFile;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        archive
            .start_file(*name, SimpleFileOptions::default())
            .expect("ZIP fixture entries should be writable");
        archive
            .write_all(bytes)
            .expect("ZIP fixture contents should be writable");
    }
    archive
        .finish()
        .expect("ZIP fixture should finish")
        .into_inner()
}

fn zip_many_empty_entries(count: usize) -> Vec<u8> {
    let entries = (0..count)
        .map(|index| (format!("entry-{index}"), Vec::new()))
        .collect::<Vec<_>>();
    let references = entries
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect::<Vec<_>>();
    zip_bytes(&references)
}

fn advance_until_cursor(emulator: &mut BrowserEmulator) {
    emulator
        .set_running(true)
        .expect("the session should start");
    let mut now = Instant::now();
    for _ in 0..3_000 {
        if emulator.session.snapshot().basic_prompt_ready {
            return;
        }
        now += Duration::from_millis(10);
        emulator.session.advance_to(now);
    }
    panic!("the ROM did not draw its BASIC cursor within the test safety bound");
}

#[cfg(all(feature = "webgpu", not(target_arch = "wasm32")))]
fn extended_webgpu_test_renderer() -> crate::emulation::extended_webgpu::ExtendedWebGpuRenderer {
    use std::sync::Arc;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("native WebGPU adapter should be available");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("WASM API vector-zoom lifecycle test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .expect("native WebGPU device should be available");
    crate::emulation::extended_webgpu::ExtendedWebGpuRenderer::new(
        Arc::new(device),
        Arc::new(queue),
        crate::emulation::CpuReferenceRenderer::WIDTH as u32,
        crate::emulation::CpuReferenceRenderer::HEIGHT as u32,
    )
    .expect("Extended WebGPU renderer should initialize")
}

#[cfg(all(feature = "webgpu", not(target_arch = "wasm32")))]
#[test]
fn vector_zoom_enable_resets_at_capture_boundary_and_disable_preserves_machine_state() {
    // What: Enabling vector zoom resets before capture, while disabling only releases its scene.
    // Why: The hard cut must establish complete history without changing reset-off behavior.
    // Arrange: Run to the BASIC prompt with Extended WebGPU active, then pause before activation.
    let mut emulator = BrowserEmulator::new().expect("resetting the machine should succeed");
    emulator
        .session
        .enable_extended_webgpu_renderer(extended_webgpu_test_renderer())
        .expect("Extended WebGPU should be available");
    advance_until_cursor(&mut emulator);
    emulator
        .set_running(false)
        .expect("the running session should pause");
    let prior_pc = emulator.snapshot().register_pc();
    assert_ne!(prior_pc, 0xbc4b);
    assert!(!emulator.session.vector_scene_capture_enabled());

    // Act: Enable capture, run the reset machine to BASIC, and then disable capture.
    emulator
        .set_vector_zoom_enabled(true)
        .expect("Extended WebGPU capture should enable");
    let enabled_snapshot = emulator.snapshot();
    let reset_pc = enabled_snapshot.register_pc();
    assert!(emulator.session.vector_scene_capture_enabled());
    advance_until_cursor(&mut emulator);
    emulator
        .set_running(false)
        .expect("the running session should pause");
    let pc_before_disable = emulator.snapshot().register_pc();
    assert_ne!(pc_before_disable, reset_pc);
    emulator
        .set_vector_zoom_enabled(false)
        .expect("scene capture should disable");
    let disabled_snapshot = emulator.snapshot();

    // Assert: Activation reset the machine; disabling kept the later, non-reset CPU state.
    assert_eq!(reset_pc, 0xbc4b);
    assert_eq!(disabled_snapshot.register_pc(), pc_before_disable);
    assert!(!emulator.session.vector_scene_capture_enabled());
}

// What: Deliver a browser key to the keyboard PIA before the machine draws its cursor.
// Why: BASIC readiness must not cause the host adapter to discard physical-keyboard input.
#[test]
fn browser_input_reaches_keyboard_pia_before_cursor_readiness() {
    // Arrange: Start the session before the ROM has drawn its cursor.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    emulator
        .set_running(true)
        .expect("the session should start");
    let starting_snapshot = emulator.snapshot();
    assert!(starting_snapshot.running());
    assert!(!starting_snapshot.basic_prompt_ready());
    emulator
        .session
        .configure_keyboard_pia_for_test()
        .expect("keyboard PIA ports should be configured");

    // Act: Submit a browser key while the running machine has not yet emitted the cursor glyph.
    emulator
        .key_down(1, b'A', false, false, false)
        .expect("the browser event should be handled");
    let snapshot = emulator.snapshot();
    assert!(!snapshot.basic_prompt_ready());
    let (key_port, _) = emulator
        .session
        .keyboard_pia_for_test()
        .expect("keyboard PIA ports should remain readable");

    // Assert: The translated key reaches the PIA even though BASIC readiness remains false.
    assert!(emulator.has_held_key());
    assert_eq!(key_port & 0x7f, b'A');
    assert!(snapshot.input_notice().is_none());
    emulator
        .release_input()
        .expect("the browser key should be released");
    assert!(!emulator.has_held_key());
}

// What: Route paired printable text and event-time Shift through the WASM session to the keyboard PIA.
// Why: Browser mapping is useful only if its translated code and Shift inversion reach the machine.
#[test]
fn browser_text_mapping_reaches_keyboard_pia() {
    // Arrange: Wait for the cursor, configure the keyboard PIA, and map shifted Digit2 to Tek @.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    advance_until_cursor(&mut emulator);
    emulator
        .session
        .configure_keyboard_pia_for_test()
        .expect("keyboard PIA ports should be configured");
    let mapping =
        map_browser_key("2", Some("@".to_owned())).expect("paired printable text should map");

    // Act: Forward the browser event's key, inversion, and held Shift modifier.
    emulator
        .key_down(1, mapping.code(), mapping.invert_shift(), true, false)
        .expect("the translated key should enter the session");
    let (key_port, modifier_port) = emulator
        .session
        .keyboard_pia_for_test()
        .expect("keyboard PIA ports should be readable");

    // Assert: The PIA sees @ with inverted Shift, which cancels the browser's held Shift line.
    assert_eq!(key_port, b'@' | 0x80);
    assert_eq!(modifier_port & 3, 3);

    // Act: Report browser focus loss, then read the PIA again.
    emulator
        .release_input()
        .expect("focus loss should release the translated key");
    let (key_port, modifier_port) = emulator
        .session
        .keyboard_pia_for_test()
        .expect("keyboard PIA ports should remain readable");

    // Assert: Focus loss clears key ownership and returns both modifier lines to idle.
    assert_eq!(key_port, 0x80);
    assert_eq!(modifier_port & 3, 3);
}

// What: Route a front-panel press through the WASM session to the keyboard PIA.
// Why: Browser panel buttons must preserve their Tek key code instead of bypassing machine input.
#[test]
fn browser_panel_press_reaches_keyboard_pia() {
    // Arrange: Wait for the cursor and expose the keyboard PIA data registers.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    advance_until_cursor(&mut emulator);
    emulator
        .session
        .configure_keyboard_pia_for_test()
        .expect("keyboard PIA ports should be configured");

    // Act: Send the browser panel key and sample the PIA while the key is held.
    emulator
        .panel_press(0x71, false, false)
        .expect("the browser panel key should enter the session");
    let (key_port, _) = emulator
        .session
        .keyboard_pia_for_test()
        .expect("keyboard PIA ports should be readable");

    // Assert: The Back Space panel code reaches the PIA with its pressed bit set.
    assert_eq!(key_port, 0xf1);

    // Act: Simulate the browser surface losing focus and observe the released PIA value.
    emulator
        .release_input()
        .expect("focus loss should release panel input");
    let (key_port, _) = emulator
        .session
        .keyboard_pia_for_test()
        .expect("keyboard PIA ports should remain readable");

    // Assert: The shared session releases the panel key without leaving its code latched.
    assert_eq!(key_port, 0x80);
}

// What: Import a mixed selection with a malformed ZIP, duplicate ZIP members, and invalid names.
// Why: Browser notices and stored bytes must reflect the original selection/member order without discarding valid files.
#[test]
fn selected_import_preserves_order_partial_errors_and_canonical_exports() {
    // Arrange: Put a broken ZIP before valid files and a later invalid loose file after the ZIP batch.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    let mut selection = super::BrowserImportSelection::new();
    selection.add_file("broken.zip".to_owned(), vec![0xff]);
    selection.add_file(
        "5      ASCII   PROG Shared 999".to_owned(),
        b"FIRST".to_vec(),
    );
    let archive = zip_bytes(&[
        ("nested/5      ASCII   PROG Shared 999", b"MID"),
        ("later/5      ASCII   PROG Shared 999", b"LAST"),
        ("invalid.txt", b"ignored"),
        ("7      NEW          Later 999", b"NEXT"),
    ]);
    selection.add_file("selection.ZIP".to_owned(), archive);
    selection.add_file("invalid-late.txt".to_owned(), b"ignored".to_vec());

    // Act: Prepare the worker-facing results, then consume them through the session importer.
    selection.prepare_selected_files();
    assert_eq!(selection.len(), 7);
    assert!(selection.result_is_error(0));
    assert!(!selection.result_is_error(1));
    assert!(!selection.result_is_error(2));
    assert!(!selection.result_is_error(3));
    assert!(!selection.result_is_error(4));
    assert!(!selection.result_is_error(5));
    assert!(!selection.result_is_error(6));
    assert_eq!(
        selection.result_file(4).map(|file| file.name()),
        Some("invalid.txt".to_owned())
    );
    assert_eq!(
        selection.result_file(6).map(|file| file.name()),
        Some("invalid-late.txt".to_owned())
    );
    emulator
        .import_selected_files(&mut selection)
        .expect("selected-file processing should complete");
    let snapshot = emulator.snapshot();
    let export = emulator
        .export_file(5)
        .expect("the later duplicate should remain exportable");
    let first_failure = snapshot
        .import_notice(1)
        .expect("the first selected archive failure should publish a notice");
    let member_failure = snapshot
        .import_notice(2)
        .expect("the invalid ZIP member should publish a notice");
    let last_failure = snapshot
        .import_notice(3)
        .expect("the final selected-file failure should publish a notice");
    let first = snapshot
        .directory_entry(0)
        .expect("the first sorted directory row should exist");
    let second = snapshot
        .directory_entry(1)
        .expect("the second sorted directory row should exist");

    // Assert: The final ZIP duplicate wins in archive order, metadata sorts numerically, and sibling errors remain visible.
    assert!(selection.is_empty());
    assert_eq!(snapshot.directory_len(), 2);
    assert_eq!(first.number(), 5);
    assert_eq!(first.file_type().as_deref(), Some("ASCII"));
    assert_eq!(first.usage().as_deref(), Some("PROG"));
    assert_eq!(first.byte_len(), 4);
    assert_eq!(second.number(), 7);
    assert_eq!(second.file_type().as_deref(), Some("NEW"));
    assert_eq!(second.usage(), None);
    assert_eq!(second.byte_len(), 4);
    assert_eq!(export.name(), "5      ASCII   PROG Shared           4");
    assert_eq!(export.bytes(), b"LAST");
    assert_eq!(snapshot.import_notice_count(), 4);
    assert!(first_failure.error());
    assert!(
        first_failure
            .message()
            .starts_with("Import failed for broken.zip:")
    );
    assert!(member_failure.error());
    assert_eq!(
        member_failure.message(),
        "Import failed for invalid.txt: invalid Flash Drive filename"
    );
    assert!(last_failure.error());
    assert_eq!(
        last_failure.message(),
        "Import failed for invalid-late.txt: invalid Flash Drive filename"
    );
}

// What: Add a second ZIP after preparation has already consumed most of the entry budget.
// Why: Incremental worker preparation must enforce one limit across the complete selection.
#[test]
fn selected_import_budget_survives_incremental_preparation() {
    // Arrange: Prepare 4095 entries, leaving room for only one entry in this selected-file batch.
    let mut selection = super::BrowserImportSelection::new();
    selection.add_file("first.zip".to_owned(), zip_many_empty_entries(4_095));
    selection.prepare_selected_files();
    assert_eq!(selection.len(), 4_095);

    // Act: Add a two-entry archive after preparation and decode it against the remaining allowance.
    selection.add_file("second.zip".to_owned(), zip_many_empty_entries(2));
    selection.prepare_selected_files();

    // Assert: The earlier files remain ordered and the second archive is rejected at the shared boundary.
    assert_eq!(selection.len(), 4_096);
    assert!(selection.result_is_error(4_095));
    assert_eq!(
        selection.result_error(4_095).as_deref(),
        Some(
            "Import failed for second.zip: ZIP archive contains 2 entries, but only 1 remain in the selected-import limit"
        )
    );
}

// What: Combine host and worker-prepared failures with already-decoded imported files.
// Why: Worker results must retain their selection order when the main session commits them.
#[test]
fn browser_host_file_errors_keep_their_selection_order() {
    // Arrange: Mix a host read failure, a worker-formatted ZIP failure, and prepared valid files.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    let mut selection = super::BrowserImportSelection::new();
    selection.add_prepared_file(
        "8      ASCII   PROG Host Read 4".to_owned(),
        b"DATA".to_vec(),
    );
    selection.add_error("unreadable.txt".to_owned(), "permission denied".to_owned());
    selection.add_prepared_error("Import failed for broken.zip: invalid ZIP archive".to_owned());
    selection.add_prepared_file(
        "8      ASCII   PROG Worker Read 5".to_owned(),
        b"READY".to_vec(),
    );

    // Act: Commit prepared worker results and read the ordered notices and both stored files.
    emulator
        .import_selected_files(&mut selection)
        .expect("host errors and valid files should be applied as one ordered batch");
    let snapshot = emulator.snapshot();
    let summary = snapshot
        .import_notice(0)
        .expect("the batch import summary should be visible");
    let notice = snapshot
        .import_notice(1)
        .expect("the host-side read failure should publish a notice");
    let worker_notice = snapshot
        .import_notice(2)
        .expect("the worker-side ZIP failure should publish a notice");
    let file = emulator
        .export_file(8)
        .expect("the following valid file should still be importable");
    let final_file = emulator
        .export_file(8)
        .expect("the later prepared worker file should remain importable");

    // Assert: Interleaved failures stay ordered, and the later duplicate worker file supplies the final bytes.
    assert!(!summary.error());
    assert_eq!(summary.message(), "2 Flash Drive file(s) imported");
    assert!(notice.error());
    assert_eq!(
        notice.message(),
        "Import failed for unreadable.txt: permission denied"
    );
    assert!(worker_notice.error());
    assert_eq!(
        worker_notice.message(),
        "Import failed for broken.zip: invalid ZIP archive"
    );
    assert_eq!(snapshot.import_notice_count(), 3);
    assert_eq!(file.bytes(), b"READY");
    assert_eq!(final_file.bytes(), b"READY");
    assert!(selection.is_empty());
}

// What: Encode a worker snapshot containing raw metadata through the shared archive writer.
// Why: Moving ZIP work off-thread must preserve exact archive bytes and unusual UTF-16 descriptions.
#[test]
fn worker_archive_snapshot_round_trips_through_shared_writer() {
    // Arrange: Restore a store with a lone surrogate and binary bytes that host filenames cannot represent.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    let archive = zip_bytes(&[
        ("metadata", br#"[3,"B","D","raw\ud83d"]"#),
        ("contents", &[0, 0xff]),
    ]);
    emulator
        .replace_flash_drive_archive(true, archive)
        .expect("the raw-metadata fixture should restore");
    let expected_archive = emulator
        .export_flash_drive_archive()
        .expect("the shared encoder should export the fixture");

    // Act: Freeze the store, reject a competing import, and transfer the snapshot to the writer.
    assert_eq!(
        emulator
            .begin_archive_export_snapshot()
            .expect("the worker snapshot should begin"),
        1
    );
    assert!(emulator.snapshot().file_io_busy());
    let mut competing_import = super::BrowserImportSelection::new();
    competing_import.add_prepared_file(
        "4      ASCII   PROG Replacement 11".to_owned(),
        b"replacement".to_vec(),
    );
    assert!(
        emulator
            .import_selected_files(&mut competing_import)
            .is_err()
    );
    assert!(!competing_import.is_empty());
    let file = emulator
        .archive_export_snapshot_file(0)
        .expect("the numbered snapshot entry should be available");
    assert_eq!(file.number(), 3);
    assert_eq!(file.file_type_code(), Some(b'B'));
    assert_eq!(file.usage_code(), Some(b'D'));
    assert_eq!(
        file.description_utf16(),
        [u16::from(b'r'), u16::from(b'a'), u16::from(b'w'), 0xd83d]
    );
    assert_eq!(file.byte_len(), 2);
    assert_eq!(file.bytes_range(1, 1).unwrap(), [0xff]);
    assert!(file.bytes_range(2, 1).is_err());
    let mut writer = BrowserArchiveWriter::new();
    writer
        .add_file(
            file.number(),
            file.file_type_code(),
            file.usage_code(),
            file.description_utf16(),
            file.bytes(),
        )
        .expect("raw snapshot metadata should be accepted");
    emulator.finish_archive_export_snapshot();
    let worker_archive = writer
        .finish()
        .expect("the shared worker encoder should finish the ZIP");

    // Assert: The snapshot releases the file-I/O gate and preserves the exact shared archive representation.
    assert!(!emulator.snapshot().file_io_busy());
    assert_eq!(worker_archive, expected_archive);
}

// What: Cancel and reject archive replacement before restoring a valid whole-storage archive.
// Why: Cancellation and malformed archives must preserve the existing store, while valid archives retain raw metadata and bytes.
#[test]
fn archive_restore_is_confirmed_atomic_and_round_trips_raw_metadata() {
    // Arrange: Seed an old file and build a whole-store archive with an unpaired UTF-16 surrogate.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    emulator.session.import_files(
        vec![FlashDriveFile {
            name: "9      ASCII   TEXT Old 999".to_owned(),
            bytes: b"old".to_vec(),
        }],
        Vec::new(),
    );
    let old_export = emulator
        .export_file(9)
        .expect("the old file should be exportable");
    let old_archive = emulator
        .export_flash_drive_archive()
        .expect("the old store should be archivable");
    let replacement = zip_bytes(&[
        ("metadata", br#"[3,"B","D","raw\ud83d"]"#),
        ("contents", &[0, 0xff]),
    ]);

    // Act: Decline replacement, reject malformed input, then confirm and export the restored store.
    assert_eq!(
        emulator
            .replace_flash_drive_archive(false, vec![0xff])
            .expect("declining replacement is not an error"),
        None
    );
    assert_eq!(
        emulator
            .export_flash_drive_archive()
            .expect("declined replacement leaves the whole old store"),
        old_archive
    );
    assert_eq!(
        emulator
            .export_file(9)
            .expect("declined replacement leaves the old file")
            .bytes(),
        old_export.bytes()
    );
    assert!(
        emulator
            .replace_flash_drive_archive(true, vec![0xff])
            .is_err()
    );
    let failure_snapshot = emulator.snapshot();
    let restore_failure = failure_snapshot
        .import_notice(0)
        .expect("a failed restore should publish an import notice");
    assert!(restore_failure.error());
    assert_eq!(
        emulator
            .export_flash_drive_archive()
            .expect("failed replacement leaves the whole old store"),
        old_archive
    );
    assert_eq!(
        emulator
            .export_file(9)
            .expect("invalid replacement is atomic")
            .bytes(),
        old_export.bytes()
    );
    emulator
        .replace_flash_drive_archive(true, replacement)
        .expect("the confirmed archive should replace the store");
    let first_archive = emulator
        .export_flash_drive_archive()
        .expect("the restored store should export as an archive");
    let mut restored = BrowserEmulator::new().expect("a second machine should reset");
    restored
        .replace_flash_drive_archive(true, first_archive.clone())
        .expect("the exported archive should be importable");
    let second_archive = restored
        .export_flash_drive_archive()
        .expect("the second store should export as an archive");
    let snapshot = restored.snapshot();
    let mut archive = ZipArchive::new(Cursor::new(&second_archive))
        .expect("the restored archive should be a valid ZIP");
    let mut contents = Vec::new();
    archive
        .by_index(1)
        .expect("the first content entry should exist")
        .read_to_end(&mut contents)
        .expect("the binary payload should be readable");

    // Assert: Replacement preserves rollback, archive bytes, binary payload, and the lone surrogate.
    assert_eq!(first_archive, second_archive);
    assert_eq!(snapshot.directory_len(), 1);
    assert_eq!(
        snapshot.directory_entry(0).unwrap().description_utf16(),
        [u16::from(b'r'), u16::from(b'a'), u16::from(b'w'), 0xd83d]
    );
    assert_eq!(snapshot.directory_entry(0).unwrap().byte_len(), 2);
    assert_eq!(contents, [0, 0xff]);
}

// What: Clear the complete Flash Drive store through the browser API.
// Why: Clearing must remove every file and remain blocked while an archive export snapshot is active.
#[test]
fn clear_flash_drive_removes_files_and_respects_archive_snapshots() {
    // Arrange: Store two numbered files and confirm both appear in the browser snapshot.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    emulator.session.import_files(
        vec![
            FlashDriveFile {
                name: "4      ASCII   PROG First 1".to_owned(),
                bytes: b"END".to_vec(),
            },
            FlashDriveFile {
                name: "6      ASCII   PROG Second 2".to_owned(),
                bytes: b"STOP".to_vec(),
            },
        ],
        Vec::new(),
    );
    assert_eq!(emulator.snapshot().directory_len(), 2);

    // Act: Reject a clear while the archive snapshot owns stable directory data, then clear afterward.
    emulator
        .begin_archive_export_snapshot()
        .expect("the stored files should form an archive snapshot");
    assert!(emulator.clear_flash_drive().is_err());
    emulator.finish_archive_export_snapshot();
    emulator
        .clear_flash_drive()
        .expect("the store should clear after the snapshot is released");

    // Assert: Both files are absent and the browser session has released its file-I/O gate.
    let snapshot = emulator.snapshot();
    assert_eq!(snapshot.directory_len(), 0);
    assert!(emulator.export_file(4).is_err());
    assert!(emulator.export_file(6).is_err());
    assert!(!snapshot.file_io_busy());
}

// What: Exercise Auto Load and launch gates through the browser-facing API.
// Why: Browser controls must use readiness detected by the shared machine display events.
#[test]
fn auto_load_and_program_launch_require_detected_prompt_readiness() {
    // Arrange: Store one eligible ASCII PROG and leave the emulator paused without readiness.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    emulator.session.import_files(
        vec![FlashDriveFile {
            name: "1      ASCII   PROG Program 12".to_owned(),
            bytes: b"10 END\r\r".to_vec(),
        }],
        Vec::new(),
    );

    // Act: Reject both operations while paused, then start and reject launch before cursor detection.
    assert!(emulator.auto_load().is_err());
    let auto_load_failure = emulator.snapshot();
    assert!(emulator.launch_file(1).is_err());
    let paused_snapshot = emulator.snapshot();
    let paused_notice = paused_snapshot
        .program_launch_notice()
        .expect("a paused launch rejection should be visible");
    assert!(paused_notice.error());
    assert_eq!(paused_snapshot.program_launch_file(), None);
    emulator
        .set_running(true)
        .expect("the session should start");
    assert!(emulator.launch_file(1).is_err());
    let unconfirmed_snapshot = emulator.snapshot();
    let unconfirmed_notice = unconfirmed_snapshot
        .program_launch_notice()
        .expect("a readiness rejection should be visible");
    assert!(unconfirmed_notice.error());
    assert_eq!(
        unconfirmed_notice.message(),
        "Wait for the BASIC cursor or INPUT prompt before launching"
    );
    assert_eq!(unconfirmed_snapshot.program_launch_file(), None);

    // Act: Reject a legacy attempt to assert readiness, then advance until the ROM draws its cursor.
    assert!(emulator.set_basic_prompt_ready(true, 42).is_err());
    let mut now = Instant::now();
    for _ in 0..3_000 {
        if emulator.session.snapshot().basic_prompt_ready {
            break;
        }
        now += Duration::from_millis(10);
        emulator.session.advance_to(now);
    }
    let ready_snapshot = emulator.snapshot();
    assert!(ready_snapshot.basic_prompt_ready());
    assert_eq!(ready_snapshot.prompt_request(), 0);

    // Act: Preserve the legacy request ID without changing the detector's readiness decision.
    emulator
        .set_basic_prompt_ready(true, 42)
        .expect("the shared detector has already recognized the cursor");
    let confirmed_snapshot = emulator.snapshot();
    emulator
        .launch_file(1)
        .expect("the detected running session may launch the eligible program");
    let launch_snapshot = emulator.snapshot();

    // Assert: Rejections are visible, readiness is detector-owned, and launch enters the sequencer.
    assert!(
        auto_load_failure.auto_load_error().is_some(),
        "Auto Load must expose why it was rejected while paused"
    );
    assert!(confirmed_snapshot.basic_prompt_ready());
    assert_eq!(confirmed_snapshot.prompt_request(), 42);
    assert_eq!(launch_snapshot.program_launch_file(), Some(1));
    assert_eq!(
        launch_snapshot.auto_load_state(),
        BrowserAutoLoadState::Typing
    );
    assert!(launch_snapshot.program_launch_notice().is_none());
}

#[test]
fn launched_h_lines_fixture_keeps_all_strokes_after_basic_returns() {
    // What: The Flash Drive H-LINES program leaves all twenty strokes in the CPU framebuffer.
    // Why: This covers machine execution and refreshes beyond synthetic renderer operations.
    // Arrange: Import the real BASIC fixture and boot to a detected BASIC prompt.
    let mut emulator = BrowserEmulator::new().expect("the default machine should reset");
    emulator.session.import_files(
        vec![FlashDriveFile {
            name: "21     ASCII   PROG [H-LINES] 680.bas".to_owned(),
            bytes: include_bytes!(
                "browser-tests/test-scripts/21     ASCII   PROG [H-LINES] 680.bas"
            )
            .to_vec(),
        }],
        Vec::new(),
    );
    let mut now = Instant::now();
    emulator
        .set_running(true)
        .expect("the imported fixture session should start");
    emulator.session.advance_to(now);
    let mut prompt_ready = false;
    for _ in 0..3_000 {
        now += Duration::from_millis(10);
        emulator.session.advance_to(now);
        if emulator.session.snapshot().basic_prompt_ready {
            prompt_ready = true;
            break;
        }
    }
    assert!(prompt_ready, "the ROM should reach its BASIC prompt");

    // Act: Launch the imported program and wait for BASIC readiness to be detected again.
    let _ = emulator.session.take_vector_event_trace();
    emulator
        .launch_file(21)
        .expect("the imported H-LINES fixture should be launchable");
    let mut prompt_restored = false;
    let mut maximum_visible_lines = 0;
    let mut previous_visible_lines = None;
    let mut visibility_transitions = Vec::new();
    for step in 0..3_000 {
        now += Duration::from_millis(10);
        emulator.session.advance_to(now);
        let snapshot = emulator.session.snapshot();
        let visible_lines = (0..20)
            .filter(|line| {
                let y = 20 + line * 39;
                [272, 512, 751].iter().all(|x| {
                    let pixel_x = (*x + 16) as usize;
                    let pixel_y = CpuReferenceRenderer::HEIGHT - 1 - (y + 10) as usize;
                    snapshot.pixels[pixel_y * CpuReferenceRenderer::WIDTH + pixel_x] == 240
                })
            })
            .count();
        maximum_visible_lines = maximum_visible_lines.max(visible_lines);
        if previous_visible_lines != Some(visible_lines) {
            visibility_transitions.push((step * 10, visible_lines));
            previous_visible_lines = Some(visible_lines);
        }
        if snapshot.basic_prompt_ready {
            prompt_restored = true;
            break;
        }
    }
    assert!(
        prompt_restored,
        "the H-LINES program should return to BASIC"
    );
    let vector_events = emulator.session.take_vector_event_trace();

    // Act: Queue the same geometry directly without INIT/PAGE to isolate file loading and erase.
    let direct_lines: Vec<String> = std::str::from_utf8(include_bytes!(
        "browser-tests/test-scripts/21     ASCII   PROG [H-LINES] 680.bas"
    ))
    .expect("the H-LINES fixture should be UTF-8")
    .split('\r')
    .map(str::trim)
    .filter(|line| !line.is_empty())
    .map(str::to_owned)
    .collect();
    let mut direct_emulator = BrowserEmulator::new().expect("the direct machine should reset");
    let mut direct_now = Instant::now();
    direct_emulator
        .set_running(true)
        .expect("the direct machine should start");
    direct_emulator.session.advance_to(direct_now);
    let mut direct_boot_prompt_ready = false;
    for _ in 0..3_000 {
        direct_now += Duration::from_millis(10);
        direct_emulator.session.advance_to(direct_now);
        if direct_emulator.session.snapshot().basic_prompt_ready {
            direct_boot_prompt_ready = true;
            break;
        }
    }
    assert!(
        direct_boot_prompt_ready,
        "the direct machine should reach BASIC"
    );
    direct_emulator
        .queue_basic_program(direct_lines)
        .expect("the BASIC sequencer should accept the H-LINES source");
    let mut direct_prompt_restored = false;
    let mut direct_progress = Vec::new();
    for step in 0..10_000 {
        direct_now += Duration::from_millis(10);
        direct_emulator.session.advance_to(direct_now);
        let snapshot = direct_emulator.session.snapshot();
        if step % 100 == 99 {
            direct_progress.push((step + 1, direct_emulator.pending_input_transitions()));
        }
        if snapshot.basic_prompt_ready {
            direct_prompt_restored = true;
            break;
        }
    }
    let direct_snapshot = direct_emulator.session.snapshot();
    assert!(
        direct_prompt_restored,
        "the directly queued H-LINES program should return to BASIC; ready={}, running={}, auto_load={:?}, pending_input={}, runtime_error={:?}, recent_progress={:?}",
        direct_snapshot.basic_prompt_ready,
        direct_snapshot.running,
        direct_snapshot.auto_load_state,
        direct_emulator.pending_input_transitions(),
        direct_snapshot.runtime_error,
        direct_progress
    );
    let direct_vector_events = direct_emulator.session.take_vector_event_trace();
    assert_eq!(
        vector_events.len(),
        20,
        "Flash Drive loading should emit one vector event per script line: {vector_events:?}"
    );
    assert_eq!(
        direct_vector_events.len(),
        20,
        "direct BASIC input should emit one vector event per script line: {direct_vector_events:?}"
    );
    let pixels = emulator.session.snapshot().pixels.clone();

    // Assert: Each line is present at three interior points in the returned framebuffer.
    let missing_points = (0..20)
        .flat_map(|line| {
            let y = 20 + line * 39;
            [272, 512, 751].map(move |x| (x, y))
        })
        .filter(|(x, y)| {
            let pixel_x = (*x + 16) as usize;
            let pixel_y = CpuReferenceRenderer::HEIGHT - 1 - (*y + 10) as usize;
            pixels[pixel_y * CpuReferenceRenderer::WIDTH + pixel_x] != 240
        })
        .collect::<Vec<_>>();
    let lit_rows = pixels
        .chunks_exact(CpuReferenceRenderer::WIDTH)
        .enumerate()
        .filter(|(_, row)| row.iter().any(|pixel| *pixel == 240))
        .map(|(row, _)| row)
        .collect::<Vec<_>>();
    assert!(
        missing_points.is_empty(),
        "the returned H-LINES framebuffer omitted these probe points: {missing_points:?}; lit rows: {lit_rows:?}; maximum complete lines observed: {maximum_visible_lines}; visibility transitions: {visibility_transitions:?}"
    );
}
