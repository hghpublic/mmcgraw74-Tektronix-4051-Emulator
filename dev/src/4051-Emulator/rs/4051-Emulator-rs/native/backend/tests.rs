//! Exercises native preparation and worker publication without opening a window.

use super::*;
use std::{sync::mpsc, time::Duration};
use tek4051_emulator::{
    emulation::{SPEED_FACTORS, display::DisplayEvent, runtime::HostRuntime},
    emulation_input::{InputAction, InputFrame, KeyModifiers, ManualKeyId},
    machine::{cpu, gpib::FlashDriveFile},
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

fn wait_for(
    backend: &mut WorkerBackend,
    mut predicate: impl FnMut(&Snapshot) -> bool,
) -> Arc<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(snapshot) = backend.snapshot().unwrap()
            && predicate(&snapshot.snapshot)
        {
            return snapshot.snapshot;
        }

        assert!(
            Instant::now() < deadline,
            "worker did not publish the expected state"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn wait_for_workload(backend: &mut WorkerBackend) -> Arc<[WorkloadSample]> {
    wait_for_workload_publication(backend).workload_samples
}

fn wait_for_workload_publication(backend: &mut WorkerBackend) -> PresentedSnapshot {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(snapshot) = backend.snapshot().unwrap()
            && !snapshot.workload_samples.is_empty()
        {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not publish workload samples"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn report_workload(factor: u32, samples: &[WorkloadSample], skipped_percent: f64) {
    assert!(
        !samples.is_empty(),
        "workload sample window must not be empty"
    );
    let loads: Vec<_> = samples
        .iter()
        .map(|sample| sample.workload_percent)
        .collect();
    let mut sorted = loads.clone();
    sorted.sort_by(f32::total_cmp);
    let average = loads.iter().sum::<f32>() / loads.len() as f32;
    let p95 = sorted[(sorted.len() * 95).div_ceil(100).saturating_sub(1)];
    let peak = sorted[sorted.len() - 1];
    let over_budget = loads.iter().filter(|sample| **sample >= 100.0).count();
    let over_budget_percent = over_budget as f32 * 100.0 / loads.len() as f32;
    eprintln!(
        "x{factor}: catch-up skipped {skipped_percent:.1}%; advance average {average:.1}%, p95 {p95:.1}%, peak {peak:.1}%; over-budget ticks {over_budget_percent:.1}% ({} samples)",
        samples.len()
    );
}

// What: Keep only recent tick-budget measurements, including values above saturation.
// Why: The graph must remain bounded without disguising worker overruns.
#[test]
fn workload_history_is_bounded_and_preserves_over_budget_samples() {
    // Arrange: Seed one under-budget sample before filling the rolling window with overruns.
    let mut workload = WorkerWorkload::new();
    workload.record(Duration::from_millis(5));

    // Act: Record one full history window at 150% of the 10 ms budget.
    for _ in 0..WORKLOAD_HISTORY_CAPACITY {
        workload.record(Duration::from_millis(15));
    }
    let samples = workload.snapshot();

    // Assert: The old sample was evicted and every retained value still exposes saturation.
    assert_eq!(samples.len(), WORKLOAD_HISTORY_CAPACITY);
    assert!(
        samples
            .iter()
            .all(|sample| sample.workload_percent == 150.0)
    );

    // Act: Clear history as a new speed measurement begins.
    workload.clear();

    // Assert: No measurements from the previous speed remain.
    assert!(workload.snapshot().is_empty());
}

// What: Clear workload history when the worker applies a speed command.
// Why: A graph window should describe one selected speed rather than mix experiments.
#[test]
fn speed_change_resets_workload_history() {
    // Arrange: Prepare a session and a workload window containing a prior measurement.
    let mut state = EmulationState::new(Machine::default());
    let mut workload = WorkerWorkload::new();
    workload.record(Duration::from_millis(5));

    // Act: Reset graph history and apply the next supported speed as the worker does.
    let command = Command::Speed(1);
    clear_workload_if_speed_command(&command, &mut workload);
    apply_worker_command(&mut state, command, Instant::now()).unwrap();

    // Assert: The new speed is active and only its future ticks will appear in the graph.
    assert_eq!(state.snapshot().speed_index, 1);
    assert!(workload.snapshot().is_empty());
}

#[test]
fn presentation_tracks_neutral_revisions_without_recoloring_metadata_changes() {
    // What: Native images match their neutral pixels and only change with display revisions.
    // Why: Metadata polling must avoid color conversion, while old publications stay immutable.
    // Arrange: Prepare the initial dark view through the production inline/native adapter.
    let mut backend = InlineBackend::new(counting_machine());
    let initial = backend.snapshot().unwrap().unwrap();

    // Act: Publish a setting change, then erase the display and publish its new revision twice.
    backend.command(Command::CapsLock(true)).unwrap();
    let metadata = backend.snapshot().unwrap().unwrap();
    backend.state.data.machine.erase_display();
    let changed = backend.snapshot().unwrap().unwrap();
    let repeated = backend.snapshot().unwrap().unwrap();

    // Assert: Each image matches its own full grayscale frame, dimensions and green palette.
    for view in [&initial, &metadata, &changed, &repeated] {
        assert_eq!(
            view.image.size,
            [CpuReferenceRenderer::WIDTH, CpuReferenceRenderer::HEIGHT]
        );
        assert_eq!(view.image.pixels.len(), view.snapshot.pixels.len());
        assert!(
            view.image
                .pixels
                .iter()
                .zip(view.snapshot.pixels.iter())
                .all(|(&color, &intensity)| color == egui::Color32::from_rgb(0, intensity, 0))
        );
    }
    assert!(metadata.snapshot.caps_lock);
    assert!(Arc::ptr_eq(&initial.image, &metadata.image));
    assert!(!Arc::ptr_eq(&metadata.image, &changed.image));
    assert!(Arc::ptr_eq(&changed.image, &repeated.image));
    assert_ne!(initial.snapshot.revision, changed.snapshot.revision);
    assert!(
        initial
            .image
            .pixels
            .iter()
            .all(|&color| color == egui::Color32::BLACK)
    );
    assert!(
        changed
            .image
            .pixels
            .iter()
            .all(|&color| color == egui::Color32::from_rgb(0, 255, 0))
    );
}

// What: Defer full CPU-image conversion while WebGPU owns presentation and refresh on fallback.
// Why: The active GPU texture already carries display changes, so recoloring every pixel is redundant.
#[test]
fn webgpu_presentation_reuses_cpu_fallback_until_renderer_fails() {
    // Arrange: Prepare a cached CPU image, then publish changed pixels from an active GPU renderer.
    let mut inline = InlineBackend::new(counting_machine());
    let initial = inline.snapshot().unwrap().unwrap();
    let mut presentation = DisplayPresentation::default();
    let initial_fallback = presentation.prepare(Arc::clone(&initial.snapshot));
    let mut active_snapshot = (*initial.snapshot).clone();
    active_snapshot.revision = active_snapshot.revision.wrapping_add(1);
    active_snapshot.pixels = Arc::from(vec![u8::MAX; initial.snapshot.pixels.len()]);
    active_snapshot.webgpu_active = true;

    // Act: Prepare active-GPU and fallback publications with the same changed CPU pixels.
    let active = presentation.prepare(Arc::new(active_snapshot.clone()));
    active_snapshot.webgpu_active = false;
    active_snapshot.render_error = Some("test renderer failure".to_owned());
    let fallback = presentation.prepare(Arc::new(active_snapshot));

    // Assert: GPU mode reuses the old cache, while fallback materializes the current CPU frame.
    assert!(Arc::ptr_eq(&initial_fallback.image, &active.image));
    assert!(!Arc::ptr_eq(&active.image, &fallback.image));
    assert!(
        fallback
            .image
            .pixels
            .iter()
            .all(|&color| color == egui::Color32::from_rgb(0, u8::MAX, 0))
    );
}

#[test]
fn native_file_effects_replace_from_archives_and_preserve_storage_on_failure() {
    // What: Native archive replacement succeeds, while read/write failures preserve its new store.
    // Why: Moving execution out of the session must preserve commit semantics and nonfatal I/O errors.
    // Arrange: Retain an old program and write a valid replacement archive under the workspace.
    let directory = tempfile::Builder::new()
        .prefix(".native-file-effects-")
        .tempdir_in(".")
        .unwrap();
    let mut backend = InlineBackend::new(counting_machine());
    let bytes = b"10 PRINT 1\r";
    backend
        .command(Command::ImportFiles(vec![FlashDriveFile {
            name: "7      ASCII   PROG Original 999".to_owned(),
            bytes: bytes.to_vec(),
        }]))
        .unwrap();
    let mut replacement = Machine::default();
    replacement.import_flash_files(&[FlashDriveFile {
        name: "9      ASCII   PROG Replacement 999".to_owned(),
        bytes: bytes.to_vec(),
    }])[0]
        .as_ref()
        .unwrap();
    let archive = directory.path().join("replacement.zip");
    let before = replacement.flash_drive_directory();
    let replacement_session = EmulationState::new(replacement);
    super::super::host_io::write_flash_storage_archive_to_path(&archive, &replacement_session)
        .unwrap();
    let missing = directory.path().join("missing.zip");

    // Act: Replace the store through the same effect executor used by the native worker.
    backend.command(Command::ImportArchive(archive)).unwrap();
    let replaced = backend.snapshot().unwrap().unwrap().snapshot;

    // Assert: Successful completion replaces the old selection and reports the committed store.
    assert!(!replaced.import_notices[0].0);
    assert_eq!(replaced.directory, before);
    assert!(backend.state.data.machine.export_flash_file(7).is_err());

    // Act: Execute a failed read and then a failed write through that native command handler.
    backend
        .command(Command::ImportArchive(missing.clone()))
        .unwrap();
    let imported = backend.snapshot().unwrap().unwrap().snapshot;
    backend
        .command(Command::ExportFile {
            number: 9,
            path: directory.path().to_path_buf(),
        })
        .unwrap();
    let exported = backend.snapshot().unwrap().unwrap().snapshot;

    // Assert: Both errors are explicit, replacement bytes/metadata survive, and execution stays paused.
    assert!(imported.import_notices[0].0);
    assert!(imported.import_notices[0].1.contains("cannot open"));
    assert!(
        imported.import_notices[0]
            .1
            .contains(&missing.display().to_string())
    );
    let notice = exported.export_notice.as_ref().unwrap();
    assert!(notice.0);
    assert!(notice.1.contains("cannot write"));
    assert_eq!(exported.directory, before);
    assert_eq!(
        backend
            .state
            .data
            .machine
            .export_flash_file(9)
            .unwrap()
            .bytes,
        bytes
    );
    assert!(!exported.running);
    assert!(exported.runtime_error.is_none());
}

#[test]
fn worker_archive_effects_complete_before_followup_commands_and_keep_execution_running() {
    // What: The real worker completes archive effects before later commands and survives I/O failure.
    // Why: Inline execution alone cannot validate modal restoration, publication or continued CPU work.
    // Arrange: Run a RAM loop with a numbered file and workspace-local export destinations.
    let directory = tempfile::Builder::new()
        .prefix(".worker-file-effects-")
        .tempdir_in(".")
        .unwrap();
    let mut machine = counting_machine();
    let bytes = b"10 PRINT \"WORKER\"\r";
    machine.import_flash_files(&[FlashDriveFile {
        name: "7      ASCII   PROG Worker 999".to_owned(),
        bytes: bytes.to_vec(),
    }])[0]
        .as_ref()
        .unwrap();
    let expected = machine.flash_drive_directory();
    let archive = directory.path().join("export.zip");
    let mut backend = WorkerBackend::start(machine).unwrap();
    backend.command(Command::Run(true)).unwrap();
    wait_for(&mut backend, |snapshot| {
        snapshot.running && snapshot.registers.ix != 0
    });

    // Act: Export on the worker and queue a state change immediately behind it.
    backend
        .command(Command::ExportArchive(archive.clone()))
        .unwrap();
    backend.command(Command::CapsLock(true)).unwrap();
    let exported = wait_for(&mut backend, |snapshot| snapshot.caps_lock);
    let mut imported = Machine::default();
    imported
        .import_flash_storage_archive(std::fs::File::open(&archive).unwrap())
        .unwrap();

    // Assert: The followup sees completed export, restored modal state and exact archived storage.
    assert!(!exported.export_notice.as_ref().unwrap().0);
    assert!(
        exported
            .export_notice
            .as_ref()
            .unwrap()
            .1
            .contains("archive exported:")
    );
    assert!(!exported.busy);
    assert!(exported.running);
    assert!(exported.runtime_error.is_none());
    assert_eq!(imported.flash_drive_directory(), expected);
    assert_eq!(imported.export_flash_file(7).unwrap().bytes, bytes);

    // Act: Fail an archive write while running, then wait for subsequent CPU progress.
    backend
        .command(Command::ExportArchive(directory.path().to_path_buf()))
        .unwrap();
    let failed = wait_for(&mut backend, |snapshot| {
        snapshot
            .export_notice
            .as_ref()
            .is_some_and(|(error, _)| *error)
    });
    let progressed = wait_for(&mut backend, |snapshot| {
        snapshot.registers.ix != failed.registers.ix
    });

    // Assert: The failure is a notice, not a stopped runtime or a latched modal suspension.
    assert!(
        failed
            .export_notice
            .as_ref()
            .unwrap()
            .1
            .contains("cannot create")
    );
    assert!(!failed.busy);
    assert!(failed.running && progressed.running);
    assert!(progressed.runtime_error.is_none());
    assert_eq!(progressed.directory, expected);

    // Act: Export the retained numbered payload after the failure through the same worker.
    let file = directory.path().join("retained.txt");
    backend
        .command(Command::ExportFile {
            number: 7,
            path: file.clone(),
        })
        .unwrap();
    let retained = wait_for(&mut backend, |snapshot| {
        snapshot
            .export_notice
            .as_ref()
            .is_some_and(|(error, message)| !*error && message.contains("file 7 exported:"))
    });
    backend.join().unwrap();

    // Assert: Failed archive creation changed neither stored bytes nor the running state.
    assert_eq!(std::fs::read(file).unwrap(), bytes);
    assert_eq!(retained.directory, expected);
    assert!(retained.running);
    assert!(retained.runtime_error.is_none());
}

#[test]
fn worker_advances_without_gui_advances() {
    // What: execution progresses after Run without any GUI advance calls.
    // Why: slow or absent repaints must not own the emulation clock.
    // Arrange: use a masked-interrupt RAM loop that increments the index register.
    let mut backend = WorkerBackend::start(counting_machine()).unwrap();

    // Act: start once, then leave the client idle across several scheduling intervals.
    backend.command(Command::Run(true)).unwrap();
    thread::sleep(Duration::from_millis(80));
    let snapshot = wait_for(&mut backend, |snapshot| snapshot.registers.ix != 0);

    // Assert: the worker performed CPU work without the GUI supplying elapsed time.
    assert!(snapshot.running);
    assert!(snapshot.runtime_error.is_none());
    assert_ne!(snapshot.registers.ix, 0);
    backend.join().unwrap();
}

// What: Report worker advancement cost and skipped wall time at every supported speed.
// Why: Comparing tick-budget use across speeds reveals when emulation work saturates the worker.
#[test]
#[ignore = "manual performance probe; run with --ignored --nocapture"]
fn reports_native_worker_catch_up_by_speed() {
    // Arrange: Run the ROM-backed machine through the same worker and image-presentation path as the app.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut backend = WorkerBackend::start(machine).unwrap();
    backend.command(Command::Run(true)).unwrap();

    // Act: Measure a fixed wall-time window after each speed selection is published.
    for (index, factor) in SPEED_FACTORS.iter().enumerate() {
        backend.command(Command::Speed(index)).unwrap();
        let before = wait_for(&mut backend, |snapshot| {
            snapshot.running && snapshot.speed_index == index
        });
        let started = Instant::now();
        thread::sleep(Duration::from_secs(1));
        let after = wait_for(&mut backend, |snapshot| {
            snapshot.running
                && snapshot.speed_index == index
                && (snapshot.registers.pc != before.registers.pc
                    || snapshot.coalesced_time > before.coalesced_time)
        });
        let wall_time = started.elapsed();
        let skipped = after.coalesced_time.saturating_sub(before.coalesced_time);
        let skipped_percent = skipped.as_secs_f64() / wall_time.as_secs_f64() * 100.0;
        let workload_samples = wait_for_workload(&mut backend);
        report_workload(*factor, &workload_samples, skipped_percent);

        // Assert: The worker stayed active and execution errors did not invalidate the sample.
        assert!(after.running);
        assert!(after.runtime_error.is_none());
    }

    backend.join().unwrap();
}

#[cfg(feature = "webgpu")]
fn native_webgpu_renderer() -> WebGpuVectorRenderer {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("native WebGPU adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Tektronix 4051 worker performance probe"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .expect("native WebGPU device");
    WebGpuVectorRenderer::new(
        Arc::new(device),
        Arc::new(queue),
        CpuReferenceRenderer::WIDTH as u32,
        CpuReferenceRenderer::HEIGHT as u32,
    )
    .expect("offscreen renderer should fit")
}

#[cfg(feature = "webgpu")]
fn configure_display_output(machine: &mut Machine, port: u16, control: u16, value: u8) {
    machine.write_byte(port, u8::MAX).unwrap();
    machine.write_byte(control, 0x04).unwrap();
    machine.write_byte(port, value).unwrap();
}

#[cfg(feature = "webgpu")]
fn display_workload_machine(program: &[u8], vector_mode: bool) -> Machine {
    const DISPLAY_Y_PORT_A: u16 = 0x878c;
    const DISPLAY_Y_PORT_B: u16 = 0x878e;
    const DISPLAY_Y_CONTROL_A: u16 = 0x878d;
    const DISPLAY_Y_CONTROL_B: u16 = 0x878f;
    const DISPLAY_X_PORT_A: u16 = 0x8794;
    const DISPLAY_X_PORT_B: u16 = 0x8796;
    const DISPLAY_X_CONTROL_A: u16 = 0x8795;
    const DISPLAY_X_CONTROL_B: u16 = 0x8797;

    let mut machine = Machine::default();
    machine.reset().unwrap();
    configure_display_output(&mut machine, DISPLAY_Y_PORT_A, DISPLAY_Y_CONTROL_A, 0);
    configure_display_output(&mut machine, DISPLAY_Y_PORT_B, DISPLAY_Y_CONTROL_B, 20);
    configure_display_output(&mut machine, DISPLAY_X_PORT_A, DISPLAY_X_CONTROL_A, 0x40);
    configure_display_output(&mut machine, DISPLAY_X_PORT_B, DISPLAY_X_CONTROL_B, 5);
    machine
        .write_byte(DISPLAY_X_PORT_A, if vector_mode { 0x80 } else { 0x40 })
        .unwrap();
    for (offset, byte) in program.iter().copied().enumerate() {
        machine.write_byte(0x0100 + offset as u16, byte).unwrap();
    }
    machine.cpu_mut().registers.pc = 0x0100;
    machine.cpu_mut().registers.ccr |= cpu::INTERRUPT_MASK;
    machine
}

#[cfg(feature = "webgpu")]
fn vector_workload_program(port: u16, second_port: u16) -> Vec<u8> {
    let [port_high, port_low] = port.to_be_bytes();
    let [second_high, second_low] = second_port.to_be_bytes();
    vec![
        0x86,
        5,
        0xb7,
        port_high,
        port_low,
        0x86,
        10,
        0xb7,
        second_high,
        second_low,
        0x20,
        0xf4,
    ]
}

#[cfg(feature = "webgpu")]
fn cursor_workload_program(read_address: u16) -> Vec<u8> {
    // Four instructions per loop produce one mark about every 25 x1 worker ticks.
    const DELAY_LOOP_COUNT: u16 = 14_149;
    let [read_high, read_low] = read_address.to_be_bytes();
    let [count_high, count_low] = DELAY_LOOP_COUNT.to_be_bytes();
    vec![
        0xb6, read_high, read_low, 0xce, count_high, count_low, 0x01, 0x01, 0x09, 0x26, 0xfb, 0x20,
        0xf3,
    ]
}

#[cfg(feature = "webgpu")]
fn count_cursor_probe_marks() -> usize {
    let program = cursor_workload_program(0x878c);
    let mut machine = display_workload_machine(&program, false);
    let mut runtime = HostRuntime::start(&mut machine);
    runtime.set_speed_factor(1.0).unwrap();
    runtime.clear_display_events();

    let mut mark_count = 0;
    for _ in 0..75 {
        runtime
            .advance(&mut machine, Duration::from_millis(10))
            .unwrap();
        let batch = runtime.take_display_events();
        mark_count += batch
            .events
            .iter()
            .filter(|event| matches!(event, DisplayEvent::TransientCursorMark(_)))
            .count();
    }
    mark_count
}

#[cfg(feature = "webgpu")]
fn wait_for_display_window(
    backend: &mut WorkerBackend,
    baseline: &PresentedSnapshot,
    duration: Duration,
) -> (Arc<Snapshot>, Arc<[WorkloadSample]>) {
    let end_elapsed = baseline
        .workload_samples
        .last()
        .expect("baseline publication contains workload samples")
        .elapsed_seconds
        + duration.as_secs_f64();
    let deadline = Instant::now() + duration + Duration::from_secs(3);
    loop {
        if let Some(publication) = backend.snapshot().unwrap()
            && publication
                .workload_samples
                .last()
                .is_some_and(|sample| sample.elapsed_seconds >= end_elapsed)
        {
            return (publication.snapshot, publication.workload_samples);
        }

        assert!(
            Instant::now() < deadline,
            "worker did not publish a sample through the requested measurement window"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(feature = "webgpu")]
fn report_display_workload(
    name: &str,
    samples: &[WorkloadSample],
    start_elapsed: f64,
    end_elapsed: f64,
    requested_duration: Duration,
) {
    const MIN_SAMPLES_PER_SECOND: f64 = 50.0;
    const MAX_SAMPLE_GAP_SECONDS: f64 = 0.1;

    let measured: Vec<_> = samples
        .iter()
        .filter(|sample| {
            sample.elapsed_seconds > start_elapsed && sample.elapsed_seconds <= end_elapsed
        })
        .collect();
    assert!(
        !measured.is_empty(),
        "display probe must collect measured ticks"
    );
    let loads: Vec<_> = measured
        .iter()
        .map(|sample| sample.workload_percent)
        .collect();
    let observed_duration =
        measured.last().unwrap().elapsed_seconds - measured.first().unwrap().elapsed_seconds;
    let largest_sample_gap = measured
        .windows(2)
        .map(|pair| pair[1].elapsed_seconds - pair[0].elapsed_seconds)
        .fold(0.0, f64::max);
    let minimum_sample_count =
        (requested_duration.as_secs_f64() * MIN_SAMPLES_PER_SECOND).ceil() as usize;
    assert!(
        observed_duration >= requested_duration.as_secs_f64() * 0.95,
        "{name} covered only {observed_duration:.3}s of the requested {:.3}s window",
        requested_duration.as_secs_f64()
    );
    assert!(
        largest_sample_gap <= MAX_SAMPLE_GAP_SECONDS,
        "{name} had a {largest_sample_gap:.3}s gap between workload samples"
    );
    assert!(
        loads.len() >= minimum_sample_count,
        "{name} collected only {} samples; expected at least {minimum_sample_count}",
        loads.len()
    );
    let mut sorted = loads.clone();
    sorted.sort_by(f32::total_cmp);
    let average = loads.iter().sum::<f32>() / loads.len() as f32;
    let p95 = sorted[(sorted.len() * 95).div_ceil(100).saturating_sub(1)];
    let peak = sorted[sorted.len() - 1];
    let over_budget = loads.iter().filter(|sample| **sample >= 100.0).count();
    let over_budget_percent = over_budget as f32 * 100.0 / loads.len() as f32;
    eprintln!(
        "{name}: x1 advance average {average:.1}%, p95 {p95:.1}%, peak {peak:.1}%; over-budget ticks {over_budget_percent:.1}% ({} samples over {observed_duration:.2}s of {:.1}s, max gap {largest_sample_gap:.3}s)",
        loads.len(),
        requested_duration.as_secs_f64()
    );
}

// What: Report worker advancement cost and skipped wall time with WebGPU at every speed.
// Why: GPU composition can raise emulation work enough to saturate at a lower speed.
#[cfg(feature = "webgpu")]
#[test]
#[ignore = "manual WebGPU performance probe; run with --ignored --nocapture"]
fn reports_native_webgpu_worker_catch_up_by_speed() {
    // Arrange: Start the ROM-backed worker and wait until its WebGPU renderer is active.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut backend = WorkerBackend::start(machine).unwrap();
    backend
        .enable_webgpu_renderer(native_webgpu_renderer())
        .unwrap();
    wait_for(&mut backend, |snapshot| snapshot.webgpu_active);
    backend.command(Command::Run(true)).unwrap();

    // Act: Measure the worker's skipped wall time at each supported speed factor.
    for (index, factor) in SPEED_FACTORS.iter().enumerate() {
        backend.command(Command::Speed(index)).unwrap();
        let before = wait_for(&mut backend, |snapshot| {
            snapshot.running && snapshot.speed_index == index
        });
        let started = Instant::now();
        thread::sleep(Duration::from_secs(1));
        let after = wait_for(&mut backend, |snapshot| {
            snapshot.running
                && snapshot.webgpu_active
                && snapshot.speed_index == index
                && (snapshot.registers.pc != before.registers.pc
                    || snapshot.coalesced_time > before.coalesced_time)
        });
        let wall_time = started.elapsed();
        let skipped = after.coalesced_time.saturating_sub(before.coalesced_time);
        let skipped_percent = skipped.as_secs_f64() / wall_time.as_secs_f64() * 100.0;
        let workload_samples = wait_for_workload(&mut backend);
        report_workload(*factor, &workload_samples, skipped_percent);

        // Assert: The worker stayed active and execution errors did not invalidate the sample.
        assert!(after.running);
        assert!(after.webgpu_active);
        assert!(after.runtime_error.is_none());
    }

    backend.join().unwrap();
}

// What: Compare matched x1 instruction loops with no display output, vector writes, and cursor marks.
// Why: The worker samples the complete CPU/reference-renderer/WebGPU advancement path.
#[cfg(feature = "webgpu")]
#[test]
#[ignore = "manual WebGPU drawing performance probe; run with --ignored --nocapture"]
fn reports_native_webgpu_display_workload_at_x1() {
    const RAM_PORT_A: u16 = 0x0200;
    const RAM_PORT_B: u16 = 0x0201;
    const DISPLAY_X_PORT_B: u16 = 0x8796;
    const DISPLAY_Y_PORT_A: u16 = 0x878c;
    let cases = [
        (
            "vector-control",
            vector_workload_program(RAM_PORT_A, RAM_PORT_B),
            true,
            false,
        ),
        (
            "vector-output",
            vector_workload_program(DISPLAY_X_PORT_B, DISPLAY_X_PORT_B),
            true,
            true,
        ),
        (
            "cursor-control",
            cursor_workload_program(RAM_PORT_A),
            false,
            false,
        ),
        (
            "cursor-output",
            cursor_workload_program(DISPLAY_Y_PORT_A),
            false,
            true,
        ),
    ];

    // Arrange: Matched pairs differ only in port addresses; preflight verifies cursor cadence.
    assert_eq!(
        count_cursor_probe_marks(),
        4,
        "cursor loop should issue one mark about every 25 x1 ticks"
    );
    for (name, program, vector_mode, expect_display_change) in cases {
        let mut backend = WorkerBackend::start(display_workload_machine(&program, vector_mode))
            .expect("headless worker should start");
        backend
            .enable_webgpu_renderer(native_webgpu_renderer())
            .expect("WebGPU renderer should attach to the worker");
        let initial = wait_for(&mut backend, |snapshot| snapshot.webgpu_active);
        let initial_pc = initial.registers.pc;
        backend.command(Command::Speed(0)).unwrap();
        backend.command(Command::Run(true)).unwrap();

        // Act: Run each fixture at x1 and wait until three seconds of worker samples have elapsed.
        let started = wait_for(&mut backend, |snapshot| {
            snapshot.running
                && snapshot.webgpu_active
                && snapshot.speed_index == 0
                && snapshot.registers.pc != initial_pc
        });
        let pixels_changed = initial
            .pixels
            .iter()
            .zip(started.pixels.iter())
            .any(|(before, after)| before != after);
        assert_eq!(
            pixels_changed, expect_display_change,
            "{name} did not produce its expected framebuffer change"
        );
        let baseline = wait_for_workload_publication(&mut backend);
        let measurement_duration = Duration::from_secs(3);
        let end_elapsed = baseline.workload_samples.last().unwrap().elapsed_seconds
            + measurement_duration.as_secs_f64();
        let (after, samples) =
            wait_for_display_window(&mut backend, &baseline, measurement_duration);
        report_display_workload(
            name,
            &samples,
            baseline.workload_samples.last().unwrap().elapsed_seconds,
            end_elapsed,
            measurement_duration,
        );

        // Assert: The worker completed the sample without falling back or recording a runtime error.
        assert!(started.runtime_error.is_none());
        assert!(after.running && after.webgpu_active);
        assert!(after.runtime_error.is_none());
        backend.join().unwrap();
    }
}

#[test]
fn snapshot_poll_does_not_wait_for_stalled_publication() {
    // What: the initial view is available even during contention; later polls return None.
    // Why: startup must not fail randomly or block when the worker is publishing.
    // Arrange: retain the publication lock to simulate a stalled publisher.
    let mut backend = WorkerBackend::start(counting_machine()).unwrap();
    let shared = Arc::clone(&backend.shared);
    let guard = shared.latest.lock().unwrap();
    let started = Instant::now();

    // Act: repeatedly poll through contention.
    assert!(backend.snapshot().unwrap().is_some());
    for _ in 0..100 {
        assert!(backend.snapshot().unwrap().is_none());
    }
    backend.command(Command::Run(true)).unwrap();

    // Assert: contention is reported immediately rather than waiting on that lock.
    assert!(started.elapsed() < Duration::from_millis(100));
    drop(guard);
    backend.join().unwrap();
}

#[test]
fn pause_reset_and_shutdown_are_observed_in_order() {
    // What: Reset clears detected readiness, Pause freezes execution, and shutdown exits.
    // Why: control commands must preserve lifecycle behavior off the GUI thread.
    // Arrange: Run the ROM-backed machine until its transient BASIC cursor is detected.
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut backend = WorkerBackend::start(machine).unwrap();
    backend.command(Command::Run(true)).unwrap();
    wait_for(&mut backend, |snapshot| {
        snapshot.running && snapshot.basic_prompt_ready
    });

    // Act: Reset while running, then pause and observe an idle interval.
    backend.command(Command::Reset).unwrap();
    let reset = wait_for(&mut backend, |snapshot| !snapshot.basic_prompt_ready);
    backend.command(Command::Run(false)).unwrap();
    let paused = wait_for(&mut backend, |snapshot| !snapshot.running);
    thread::sleep(Duration::from_millis(40));
    let still_paused = wait_for(&mut backend, |snapshot| !snapshot.running);
    backend.request_shutdown();
    backend.join().unwrap();

    // Assert: paused CPU state is stable, Reset preserves run state, and shutdown rejects commands.
    assert_eq!(paused.registers.ix, still_paused.registers.ix);
    assert!(reset.running);
    assert!(!reset.basic_prompt_ready);
    assert!(backend.command(Command::Run(true)).is_err());
}

#[test]
fn file_operation_acknowledgement_freezes_execution_until_completion() {
    // What: A file-operation acknowledgement publishes an already-suspended machine.
    // Why: Save As metadata and subsequent export must observe the same store boundary.
    // Arrange: Start the real worker and wait until CPU execution is observable.
    let mut backend = WorkerBackend::start(counting_machine()).unwrap();
    backend.command(Command::Run(true)).unwrap();
    wait_for(&mut backend, |snapshot| snapshot.registers.ix != 0);

    // Act: Request a uniquely identified modal boundary, then leave the GUI idle.
    backend.command(Command::BeginFileOperation(7)).unwrap();
    let frozen = wait_for(&mut backend, |snapshot| {
        snapshot.busy && snapshot.file_operation == 7
    });
    thread::sleep(Duration::from_millis(40));
    let later = wait_for(&mut backend, |snapshot| snapshot.busy);
    backend.command(Command::Modal(false)).unwrap();
    let resumed = wait_for(&mut backend, |snapshot| {
        !snapshot.busy && snapshot.registers != frozen.registers
    });
    backend.join().unwrap();

    // Assert: Modal time does not execute the CPU; completing the operation resumes it.
    assert_eq!(frozen.registers, later.registers);
    assert!(frozen.running && resumed.running);
    assert!(resumed.runtime_error.is_none());
}

#[test]
fn disconnected_receiver_is_an_explicit_client_error() {
    // What: both submission and polling reject a disconnected worker.
    // Why: silently accepting commands could strand keyboard releases.
    // Arrange: build a client whose receiving endpoint has exited.
    let mut backend = WorkerBackend::start(counting_machine()).unwrap();
    backend.join().unwrap();

    // Act: try submitting input control and polling after completion.
    let submission = backend.command(Command::Run(true));
    let polling = backend.snapshot();

    // Assert: the terminal condition remains visible on every subsequent operation.
    assert!(submission.unwrap_err().contains("disconnected"));
    assert!(polling.err().unwrap().contains("disconnected"));
}

#[test]
fn host_key_queue_is_nonblocking_ordered_and_reports_overflow() {
    // What: A stalled worker consumer does not block host enqueue, and accepted edges stay ordered.
    // Why: GUI event capture must stay responsive without silently losing accepted key transitions.
    // Arrange: Hold the worker consumer and prepare a backend with an empty shared FIFO.
    let shared = Arc::new(Shared {
        latest: Mutex::new(None),
        commands: Mutex::new(CommandQueue::default()),
        shutdown: AtomicBool::new(false),
        finished: AtomicBool::new(false),
    });
    let mut backend = WorkerBackend {
        shared: Arc::clone(&shared),
        worker: None,
        failure: None,
        initial: None,
        #[cfg(feature = "webgpu")]
        webgpu_sender: mpsc::sync_channel(1).0,
    };

    let input_command = |key: usize, pressed: bool| {
        let key = ManualKeyId(key as u64);
        let action = if pressed {
            InputAction::SequencedManualPress {
                key,
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            }
        } else {
            InputAction::SequencedManualRelease(key)
        };
        Command::Input(Box::new(InputFrame {
            actions: vec![action],
            modifiers: KeyModifiers::NONE,
            focused: true,
            reset_input: false,
        }))
    };
    let queued_key_count = MAX_PENDING_KEY_TRANSITIONS / 2;

    // Act: Hold the consumer while the producer fills the limit, then release it even on timeout.
    let (resume_sender, resume_receiver) = mpsc::channel();
    let (consumer_ready_sender, consumer_ready_receiver) = mpsc::channel();
    let producer_done = Arc::new(AtomicBool::new(false));
    let consumer_shared = Arc::clone(&shared);
    let consumer_producer_done = Arc::clone(&producer_done);
    let consumer = thread::spawn(move || {
        consumer_ready_sender
            .send(())
            .expect("the test must observe consumer readiness");
        resume_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the test must release the blocked consumer");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut delivered = Vec::new();
        let mut recovery_seen = false;
        let mut unexpected_command = false;
        let expected_delivered = (0..queued_key_count)
            .flat_map(|key| [(key as u64, true), (key as u64, false)])
            .collect::<Vec<_>>();
        loop {
            match take_worker_command(&consumer_shared)
                .expect("the worker queue must remain usable")
            {
                Some(Command::Input(frame)) if *frame == InputFrame::unfocused() => {
                    if recovery_seen || delivered != expected_delivered {
                        unexpected_command = true;
                    }
                    recovery_seen = true;
                }
                Some(Command::Input(frame)) if frame.actions.is_empty() => {
                    unexpected_command = true;
                }
                Some(Command::Input(frame)) => {
                    for action in &frame.actions {
                        match action {
                            InputAction::SequencedManualPress { key, .. } => {
                                delivered.push((key.0, true));
                            }
                            InputAction::SequencedManualRelease(key) => {
                                delivered.push((key.0, false));
                            }
                            _ => unexpected_command = true,
                        }
                    }
                }
                Some(_) => unexpected_command = true,
                None if consumer_producer_done.load(Ordering::Acquire) => break,
                None if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(1));
                }
                None => {
                    unexpected_command = true;
                    break;
                }
            }
        }
        (delivered, recovery_seen, unexpected_command)
    });
    consumer_ready_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("the consumer must be ready before host input is submitted");

    let (completed_sender, completed_receiver) = mpsc::channel();
    let producer_done_flag = Arc::clone(&producer_done);
    let producer = thread::spawn(move || {
        for key in 0..queued_key_count {
            backend.command(input_command(key, true)).unwrap();
            backend.command(input_command(key, false)).unwrap();
        }
        let overflow_error = backend.command(input_command(queued_key_count, true));
        let repeated_error = backend.command(input_command(queued_key_count, false));
        let (pending_len, pending_key_transitions, queue_overflow) = {
            let queue = backend.shared.commands.lock().unwrap();
            (
                queue.pending.len(),
                queue.pending_key_transitions,
                queue.overflow.clone(),
            )
        };
        producer_done_flag.store(true, Ordering::Release);
        completed_sender.send(()).unwrap();
        (
            backend,
            overflow_error,
            repeated_error,
            pending_len,
            pending_key_transitions,
            queue_overflow,
        )
    });
    let submitted_without_worker_progress = completed_receiver
        .recv_timeout(Duration::from_secs(2))
        .is_ok();
    resume_sender.send(()).unwrap();
    let producer_finished_after_release = submitted_without_worker_progress
        || completed_receiver
            .recv_timeout(Duration::from_secs(5))
            .is_ok();
    if !producer_finished_after_release {
        drop(producer);
        let _ = consumer.join();
        panic!("producer did not finish after the consumer was released");
    }
    let (
        backend,
        overflow_error,
        repeated_error,
        pending_len_at_overflow,
        pending_transitions_at_overflow,
        queue_overflow_at_overflow,
    ) = producer.join().unwrap();
    let (delivered, recovery_seen, unexpected_command) = consumer.join().unwrap();

    // Assert: Accepted transitions drain in order; overflow is latched without clearing them.
    assert!(submitted_without_worker_progress);
    assert_eq!(pending_len_at_overflow, PENDING_CAPACITY);
    assert_eq!(pending_transitions_at_overflow, MAX_PENDING_KEY_TRANSITIONS);
    let expected_error = format!(
        "keyboard input queue full ({MAX_PENDING_KEY_TRANSITIONS} pending key transitions); newest input frame was not queued"
    );
    assert_eq!(overflow_error.unwrap_err(), expected_error);
    assert_eq!(repeated_error.unwrap_err(), expected_error);
    assert_eq!(
        queue_overflow_at_overflow.as_deref(),
        Some(expected_error.as_str())
    );
    let queue = shared.commands.lock().unwrap();
    assert!(queue.pending.is_empty());
    assert_eq!(queue.pending_key_transitions, 0);
    assert_eq!(
        queue.processing_key_transitions,
        MAX_PENDING_KEY_TRANSITIONS
    );
    assert_eq!(
        total_pending_key_transitions(&queue),
        MAX_PENDING_KEY_TRANSITIONS
    );
    assert!(backend.failure.is_none());
    assert!(!shared.shutdown.load(Ordering::Acquire));
    assert_eq!(
        delivered,
        (0..queued_key_count)
            .flat_map(|key| [(key as u64, true), (key as u64, false)])
            .collect::<Vec<_>>()
    );
    assert!(recovery_seen);
    assert!(!unexpected_command);
}

// What: Rejects new key transitions when earlier ones are already in the sequencer.
// Why: Moving input out of the host FIFO must not make the end-to-end capacity available again.
#[test]
fn sequenced_input_counts_toward_the_end_to_end_limit() {
    // Arrange: Prepare a stopped worker backend with all but one transition slot in the sequencer.
    let shared = Arc::new(Shared {
        latest: Mutex::new(None),
        commands: Mutex::new(CommandQueue {
            sequenced_key_transitions: MAX_PENDING_KEY_TRANSITIONS - 1,
            ..CommandQueue::default()
        }),
        shutdown: AtomicBool::new(false),
        finished: AtomicBool::new(false),
    });
    let mut backend = WorkerBackend {
        shared: Arc::clone(&shared),
        worker: None,
        failure: None,
        initial: None,
        #[cfg(feature = "webgpu")]
        webgpu_sender: mpsc::sync_channel(1).0,
    };
    let input_command = |key| {
        Command::Input(Box::new(InputFrame {
            actions: vec![InputAction::SequencedManualPress {
                key: ManualKeyId(key),
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            }],
            modifiers: KeyModifiers::NONE,
            focused: true,
            reset_input: false,
        }))
    };

    // Act: Fill the final slot, then attempt one more transition.
    backend.command(input_command(1)).unwrap();
    let overflow = backend.command(input_command(2));

    // Assert: The queued transition and recovery frame remain ordered, with an explicit overflow.
    let queue = shared.commands.lock().unwrap();
    let expected_error = format!(
        "keyboard input queue full ({MAX_PENDING_KEY_TRANSITIONS} pending key transitions); newest input frame was not queued"
    );
    assert_eq!(overflow.unwrap_err(), expected_error);
    assert_eq!(
        total_pending_key_transitions(&queue),
        MAX_PENDING_KEY_TRANSITIONS
    );
    assert_eq!(queue.pending_key_transitions, 1);
    assert_eq!(
        queue.sequenced_key_transitions,
        MAX_PENDING_KEY_TRANSITIONS - 1
    );
    assert_eq!(queue.pending.len(), 2);
    assert!(
        matches!(queue.pending.back(), Some(Command::Input(frame)) if **frame == InputFrame::unfocused())
    );
    assert_eq!(queue.overflow.as_deref(), Some(expected_error.as_str()));
}

// What: Auto Load and directory-launch commands reserve capacity for their generated key taps.
// Why: Script transitions must not bypass the end-to-end limit before entering the sequencer.
#[test]
fn scripted_commands_count_toward_the_end_to_end_limit() {
    // Arrange: Check exact-fit submissions before testing the overflow boundary.
    for (command, transitions) in [
        (Command::AutoLoad, 2 * b"FIND@5:1\rOLD@5:\rRUN\r".len()),
        (
            Command::Launch(7),
            2 * "FIND@5:7\rCALL \"BOLD\",5\rRUN\r".len(),
        ),
    ] {
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            commands: Mutex::new(CommandQueue {
                sequenced_key_transitions: MAX_PENDING_KEY_TRANSITIONS - transitions,
                ..CommandQueue::default()
            }),
            shutdown: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });
        let mut backend = WorkerBackend {
            shared: Arc::clone(&shared),
            worker: None,
            failure: None,
            initial: None,
            #[cfg(feature = "webgpu")]
            webgpu_sender: mpsc::sync_channel(1).0,
        };

        // Act: Accept a script that exactly fills the remaining transition capacity.
        backend.command(command).unwrap();

        // Assert: Its reservation is retained with the queued script at the exact limit.
        let queue = shared.commands.lock().unwrap();
        assert_eq!(queue.pending_key_transitions, transitions);
        assert_eq!(
            total_pending_key_transitions(&queue),
            MAX_PENDING_KEY_TRANSITIONS
        );
        assert_eq!(queue.pending.len(), 1);
        assert!(matches!(
            queue.pending.back(),
            Some(Command::AutoLoad | Command::Launch(_))
        ));
        drop(queue);

        // Act: Submit one additional key transition while the queued script uses the full limit.
        let overflow = backend.command(Command::Input(Box::new(InputFrame {
            actions: vec![InputAction::SequencedManualPress {
                key: ManualKeyId(1),
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            }],
            modifiers: KeyModifiers::NONE,
            focused: true,
            reset_input: false,
        })));

        // Assert: The extra key overflows without removing the script or exceeding its reservation.
        let queue = shared.commands.lock().unwrap();
        let expected_error = format!(
            "keyboard input queue full ({MAX_PENDING_KEY_TRANSITIONS} pending key transitions); newest input frame was not queued"
        );
        assert_eq!(overflow.unwrap_err(), expected_error);
        assert_eq!(
            total_pending_key_transitions(&queue),
            MAX_PENDING_KEY_TRANSITIONS
        );
        assert_eq!(queue.pending_key_transitions, transitions);
        assert_eq!(queue.pending.len(), 2);
        assert!(matches!(
            queue.pending.front(),
            Some(Command::AutoLoad | Command::Launch(_))
        ));
        assert!(
            matches!(queue.pending.back(), Some(Command::Input(frame)) if **frame == InputFrame::unfocused())
        );
    }

    // Arrange: Leave fewer than one script's transitions available in an idle worker queue.
    for command in [Command::AutoLoad, Command::Launch(7)] {
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            commands: Mutex::new(CommandQueue {
                sequenced_key_transitions: MAX_PENDING_KEY_TRANSITIONS - 1,
                ..CommandQueue::default()
            }),
            shutdown: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });
        let mut backend = WorkerBackend {
            shared: Arc::clone(&shared),
            worker: None,
            failure: None,
            initial: None,
            #[cfg(feature = "webgpu")]
            webgpu_sender: mpsc::sync_channel(1).0,
        };

        // Act: Submit a script while only one transition slot remains.
        let overflow = backend.command(command);

        // Assert: The command is rejected and the recovery frame preserves the latched overflow.
        let queue = shared.commands.lock().unwrap();
        let expected_error = format!(
            "keyboard input queue full ({MAX_PENDING_KEY_TRANSITIONS} pending key transitions); newest input frame was not queued"
        );
        assert_eq!(overflow.unwrap_err(), expected_error);
        assert_eq!(
            total_pending_key_transitions(&queue),
            MAX_PENDING_KEY_TRANSITIONS - 1
        );
        assert_eq!(queue.pending.len(), 1);
        assert!(
            matches!(queue.pending.back(), Some(Command::Input(frame)) if **frame == InputFrame::unfocused())
        );
        assert_eq!(queue.pending_key_transitions, 0);
        assert_eq!(queue.overflow.as_deref(), Some(expected_error.as_str()));
    }
}

// What: A worker-delayed key retains its full press phase after it reaches the session.
// Why: Time spent before command application must not shorten a newly accepted key tap.
#[test]
fn worker_input_phase_starts_when_the_command_is_applied() {
    // Arrange: Run an idle session and choose a deterministic future acceptance timestamp.
    let mut state = EmulationState::new(counting_machine());
    apply_command(&mut state, Command::Run(true)).unwrap();
    let accepted_at = Instant::now() + Duration::from_secs(1);
    let key = ManualKeyId(1);
    let command = Command::Input(Box::new(InputFrame {
        actions: vec![
            InputAction::SequencedManualPress {
                key,
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualRelease(key),
        ],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    }));

    // Act: Apply the command at its acceptance time, then advance to just before key-up.
    apply_worker_command(&mut state, command, accepted_at).unwrap();
    state.advance_to(accepted_at + Duration::from_millis(49));

    // Assert: The queued release is still pending after only 49 ms of the active press.
    assert_eq!(state.pending_input_transitions(), 1);

    // Act: Advance the final millisecond of the minimum press phase.
    state.advance_to(accepted_at + Duration::from_millis(50));

    // Assert: The matching release has now been applied.
    assert_eq!(state.pending_input_transitions(), 0);
}

// What: Modal transitions preserve input timing before and after the suspension.
// Why: Time spent before a dialog must advance the active press, while modal time must not.
#[test]
fn worker_modal_transitions_pause_only_the_modal_interval() {
    // Arrange: Start one tap, advance 20 ms, then suspend the running session.
    let mut state = EmulationState::new(counting_machine());
    apply_command(&mut state, Command::Run(true)).unwrap();
    let pressed_at = Instant::now() + Duration::from_secs(1);
    let key = ManualKeyId(1);
    let input = Command::Input(Box::new(InputFrame {
        actions: vec![
            InputAction::SequencedManualPress {
                key,
                code: b'A',
                invert_shift: false,
                modifiers: KeyModifiers::NONE,
            },
            InputAction::SequencedManualRelease(key),
        ],
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    }));
    apply_worker_command(&mut state, input, pressed_at).unwrap();
    state.advance_to(pressed_at + Duration::from_millis(20));
    apply_worker_command(
        &mut state,
        Command::Modal(true),
        pressed_at + Duration::from_millis(20),
    )
    .unwrap();

    // Act: Resume after 80 ms of modal time, then advance to 29 ms of active time.
    apply_worker_command(
        &mut state,
        Command::Modal(false),
        pressed_at + Duration::from_millis(100),
    )
    .unwrap();
    state.advance_to(pressed_at + Duration::from_millis(129));

    // Assert: Modal time did not expire the press; 49 active milliseconds have elapsed.
    assert_eq!(state.pending_input_transitions(), 1);

    // Act: Advance the final active millisecond of the minimum press phase.
    state.advance_to(pressed_at + Duration::from_millis(130));

    // Assert: The matching key-up is applied after exactly 50 active milliseconds.
    assert_eq!(state.pending_input_transitions(), 0);
}

// What: Modal suspension preserves the release gap before a queued second tap.
// Why: Modal time must not let the next key start before its full active release interval.
#[test]
fn worker_modal_transitions_pause_release_gap_before_next_key() {
    // Arrange: Queue two taps and let the first press finish with 20 ms of release remaining.
    let mut state = EmulationState::new(counting_machine());
    apply_command(&mut state, Command::Run(true)).unwrap();
    let started_at = Instant::now() + Duration::from_secs(1);
    let first = ManualKeyId(1);
    let second = ManualKeyId(2);
    let input = Command::Input(Box::new(InputFrame {
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
    }));
    apply_worker_command(&mut state, input, started_at).unwrap();
    state.advance_to(started_at + Duration::from_millis(60));

    // Act: Suspend during the release gap, then resume after 80 ms of modal time.
    apply_worker_command(
        &mut state,
        Command::Modal(true),
        started_at + Duration::from_millis(60),
    )
    .unwrap();
    apply_worker_command(
        &mut state,
        Command::Modal(false),
        started_at + Duration::from_millis(140),
    )
    .unwrap();
    state.advance_to(started_at + Duration::from_millis(159));

    // Assert: Both edges for the second tap remain queued after 19 active release milliseconds.
    assert_eq!(state.pending_input_transitions(), 2);

    // Act: Advance the final millisecond of the remaining release interval.
    state.advance_to(started_at + Duration::from_millis(160));

    // Assert: The second tap has started, leaving only its matching key-up pending.
    assert_eq!(state.pending_input_transitions(), 1);
}

#[test]
fn repeated_empty_input_does_not_exhaust_stalled_worker_queues() {
    // What: Idle frames coalesce without losing a reset transition or a later key release.
    // Why: repeated GUI frames must not consume bounded host-queue capacity without new input.
    // Arrange: Build a backend with a worker queue that is held for inspection.
    let shared = Arc::new(Shared {
        latest: Mutex::new(None),
        commands: Mutex::new(CommandQueue::default()),
        shutdown: AtomicBool::new(false),
        finished: AtomicBool::new(false),
    });
    #[cfg(feature = "webgpu")]
    let (webgpu_sender, _webgpu_receiver) = mpsc::sync_channel(1);
    let mut backend = WorkerBackend {
        shared: Arc::clone(&shared),
        worker: None,
        failure: None,
        initial: None,
        #[cfg(feature = "webgpu")]
        webgpu_sender,
    };
    let frame = InputFrame {
        actions: Vec::new(),
        modifiers: KeyModifiers::NONE,
        focused: true,
        reset_input: false,
    };

    // Act: submit repeated idle/reset/focus frames and then an actual key release.
    for _ in 0..1_000 {
        backend
            .command(Command::Input(Box::new(frame.clone())))
            .unwrap();
    }
    // Act: Repeated input-reset requests also coalesce while native text editing owns input.
    let mut editing = frame.clone();
    editing.reset_input = true;
    for _ in 0..1_000 {
        backend
            .command(Command::Input(Box::new(editing.clone())))
            .unwrap();
    }
    // Act: Inactive frames ignore reset metadata, just as pre/post-widget focus-loss frames do.
    let mut unfocused = frame.clone();
    unfocused.focused = false;
    for _ in 0..1_000 {
        unfocused.reset_input = false;
        backend
            .command(Command::Input(Box::new(unfocused.clone())))
            .unwrap();
        unfocused.reset_input = true;
        backend
            .command(Command::Input(Box::new(unfocused.clone())))
            .unwrap();
    }
    let mut release = frame;
    release
        .actions
        .push(InputAction::ManualRelease(ManualKeyId(1)));
    backend.command(Command::Input(Box::new(release))).unwrap();

    // Assert: Duplicate frames coalesce and the first reset remains before the explicit release.
    assert!(!shared.shutdown.load(Ordering::Acquire));
    let queue = shared.commands.lock().unwrap();
    assert!(queue.pending.len() < PENDING_CAPACITY);
    let reset_index = queue.pending.iter().position(|command| {
        matches!(command, Command::Input(frame) if frame.focused && frame.reset_input && frame.actions.is_empty())
    }).expect("the transition into input reset must not coalesce with prior idle input");
    let release_index = queue
        .pending
        .iter()
        .position(|command| {
            matches!(command, Command::Input(frame) if frame.actions.contains(
                &InputAction::ManualRelease(ManualKeyId(1))
            ))
        })
        .expect("the explicit key release must remain queued");
    assert!(reset_index < release_index);
    assert!(matches!(
        queue.pending.back(),
        Some(Command::Input(frame))
            if frame.actions.last().is_some_and(|action| matches!(
                action,
                InputAction::ManualRelease(ManualKeyId(1))
            ))
    ));
    assert_eq!(queue.pending_key_transitions, 1);
    assert!(queue.overflow.is_none());
}
