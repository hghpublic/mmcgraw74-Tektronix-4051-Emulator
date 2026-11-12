use std::collections::VecDeque;

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2};

use super::super::native_backend::{Backend, InlineBackend, PresentedSnapshot, WorkloadSample};
use super::*;
use crate::emulation::session::test_support::observe_cursor_glyph;
use tek4051_emulator::{emulation::Command, emulation_input::InputAction};

struct PendingBackend {
    initial: Option<PresentedSnapshot>,
    commands: Vec<Command>,
}

impl Backend for PendingBackend {
    fn command(&mut self, command: Command) -> Result<(), String> {
        self.commands.push(command);
        Ok(())
    }

    fn snapshot(&mut self) -> Result<Option<PresentedSnapshot>, String> {
        Ok(self.initial.take())
    }

    fn advance(&mut self, _now: Instant) {}
}

struct SnapshotBackend {
    snapshots: VecDeque<PresentedSnapshot>,
}

struct FailingBackend {
    initial: Option<PresentedSnapshot>,
    error: String,
}

impl Backend for FailingBackend {
    fn command(&mut self, _command: Command) -> Result<(), String> {
        Err(self.error.clone())
    }

    fn snapshot(&mut self) -> Result<Option<PresentedSnapshot>, String> {
        Ok(self.initial.take())
    }

    fn advance(&mut self, _now: Instant) {}
}

impl Backend for SnapshotBackend {
    fn command(&mut self, _command: Command) -> Result<(), String> {
        Ok(())
    }

    fn snapshot(&mut self) -> Result<Option<PresentedSnapshot>, String> {
        Ok(self.snapshots.pop_front())
    }

    fn advance(&mut self, _now: Instant) {}
}

fn workload_series_points(output: &egui::FullOutput) -> &[Pos2] {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::Path(path)
                if path.points.len() == 6
                    && path.stroke.width == 1.5
                    && matches!(
                        &path.stroke.color,
                        egui::epaint::ColorMode::Solid(color)
                            if *color == egui::Color32::LIGHT_BLUE
                    ) =>
            {
                Some(path.points.as_slice())
            }
            _ => None,
        })
        .expect("workload series should emit its samples and trailing value")
}

// What: Skip CPU texture updates during WebGPU presentation and catch up on renderer fallback.
// Why: Uploading a full 1046x830 image for every display revision duplicates GPU work.
#[test]
fn cpu_texture_refresh_waits_for_webgpu_fallback() {
    // Arrange: Prepare initial, active-GPU, and failed-GPU publications with successive revisions.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    let initial = inline.snapshot().unwrap().unwrap();
    let mut active_snapshot = (*initial.snapshot).clone();
    active_snapshot.revision = active_snapshot.revision.wrapping_add(1);
    active_snapshot.webgpu_active = true;
    let mut fallback_snapshot = active_snapshot.clone();
    fallback_snapshot.webgpu_active = false;
    fallback_snapshot.render_error = Some("test renderer failure".to_owned());
    let fallback_revision = fallback_snapshot.revision;
    let fallback_pixels = Arc::from(vec![u8::MAX; initial.snapshot.pixels.len()]);
    fallback_snapshot.pixels = Arc::clone(&fallback_pixels);
    let fallback_image = Arc::new(egui::ColorImage::new(
        [CpuReferenceRenderer::WIDTH, CpuReferenceRenderer::HEIGHT],
        fallback_pixels
            .iter()
            .map(|&pixel| egui::Color32::from_rgb(0, pixel, 0))
            .collect(),
    ));
    let backend = SnapshotBackend {
        snapshots: VecDeque::from([
            initial.clone(),
            PresentedSnapshot {
                snapshot: Arc::new(active_snapshot),
                image: Arc::clone(&initial.image),
                workload_samples: Arc::clone(&initial.workload_samples),
            },
            PresentedSnapshot {
                snapshot: Arc::new(fallback_snapshot),
                image: fallback_image,
                workload_samples: Arc::clone(&initial.workload_samples),
            },
        ]),
    };
    let mut app = Tek4051App::with_backend(&creation, backend).unwrap();
    let initial_texture_revision = app.display_revision;

    // Act: Refresh once with the GPU frame, then again after the worker falls back to CPU pixels.
    app.refresh();
    let active_texture_revision = app.display_revision;
    app.refresh();

    // Assert: CPU texture revision stays cached during GPU use and catches up on fallback.
    assert_eq!(active_texture_revision, initial_texture_revision);
    assert_eq!(app.display_revision, fallback_revision);
}

// What: Show measured samples, saturation threshold, and continuous scrolling between updates.
// Why: Users need a readable comparison that does not jump only when a worker sample arrives.
#[test]
fn workload_graph_displays_samples_and_budget_threshold() {
    // Arrange: Publish five measurements spanning three elapsed seconds, including the exact budget boundary.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    let initial = inline.snapshot().unwrap().unwrap();
    let graph_snapshot = PresentedSnapshot {
        workload_samples: vec![
            WorkloadSample {
                elapsed_seconds: 3.0,
                workload_percent: 50.0,
            },
            WorkloadSample {
                elapsed_seconds: 3.04,
                workload_percent: 100.0,
            },
            WorkloadSample {
                elapsed_seconds: 3.08,
                workload_percent: 125.0,
            },
            WorkloadSample {
                elapsed_seconds: 3.12,
                workload_percent: 75.0,
            },
            WorkloadSample {
                elapsed_seconds: 3.16,
                workload_percent: 125.0,
            },
        ]
        .into(),
        ..initial
    };
    let backend = SnapshotBackend {
        snapshots: VecDeque::from([graph_snapshot]),
    };
    let mut app = Tek4051App::with_backend(&creation, backend).unwrap();

    // Act: Render the panel and inspect its labels, series path, threshold, and unlabeled second ticks.
    let frame_at = Instant::now();
    app.workload_received_at = frame_at;
    let output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
            ..Default::default()
        },
        |context| app.update_at(context, frame_at),
    );
    let text: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.job.text.to_string()),
            _ => None,
        })
        .collect();
    let points = workload_series_points(&output);
    let mut tick_marks: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::GRAY
                    && stroke.width == 1.5
                    && (points[0].x - points[1].x).abs() < 0.1
                    && ((points[1].y - points[0].y) - 8.0).abs() < 0.1 =>
            {
                Some((points[0].x, points[0].y))
            }
            _ => None,
        })
        .collect();
    tick_marks.sort_by(|first, second| first.0.total_cmp(&second.0));
    let threshold_y = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::LIGHT_RED
                    && (points[0].y - points[1].y).abs() < 0.1 =>
            {
                Some((points[0].y, points[0].x, points[1].x))
            }
            _ => None,
        })
        .expect("100% threshold should be drawn as a horizontal line");

    // Assert: The graph keeps sample order, draws one tick per elapsed second, and labels no ticks.
    assert!(text.iter().any(|label| label == "Emulation workload"));
    assert!(
        text.iter()
            .any(|label| label.contains("peak 125%") && label.contains("100% = one 10 ms budget"))
    );
    assert!(points[..5].windows(2).all(|pair| pair[0].x < pair[1].x));
    let plot_width = (tick_marks[1].0 - tick_marks[0].0) * 3.0;
    for ((point, elapsed_seconds), workload_percent) in points
        .iter()
        .take(5)
        .zip([3.0, 3.04, 3.08, 3.12, 3.16])
        .zip([50.0, 100.0, 125.0, 75.0, 125.0])
    {
        let expected_x = tick_marks[2].0 + plot_width * ((elapsed_seconds - 3.0) / 3.0) as f32;
        let expected_y =
            points[0].y + (points[2].y - points[0].y) * ((workload_percent - 50.0) / 75.0);
        assert!((point.x - expected_x).abs() < 0.1);
        assert!((point.y - expected_y).abs() < 0.1);
    }
    assert!((threshold_y.0 - points[1].y).abs() < 0.1);
    let expected_plot_left = tick_marks[0].0 - plot_width * ((1.0 - 0.16) / 3.0) as f32;
    assert!((threshold_y.1 - expected_plot_left).abs() < 0.1);
    assert!((threshold_y.2 - points[5].x).abs() < 0.1);
    assert!((points[5].y - points[4].y).abs() < 0.1);
    assert_eq!(tick_marks.len(), 3);
    assert!(
        tick_marks
            .windows(3)
            .all(|ticks| ((ticks[1].0 - ticks[0].0) - (ticks[2].0 - ticks[1].0)).abs() < 0.1)
    );
    for (index, tick) in tick_marks.iter().enumerate() {
        let elapsed_seconds = 1.0 + index as f64;
        let expected_x = tick_marks[2].0 + plot_width * ((elapsed_seconds - 3.0) / 3.0) as f32;
        assert!(
            (tick.0 - expected_x).abs() < 0.1,
            "tick {tick:?} should align at x={expected_x}, sample path={points:?}"
        );
    }
    assert!(
        output.shapes.iter().all(|shape| match &shape.shape {
            Shape::Text(text) => text.pos.y < tick_marks[0].1,
            _ => true,
        }),
        "one-second ticks must not have text labels"
    );

    // Act: Render again between worker samples while the published data stays unchanged.
    let next_output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
            ..Default::default()
        },
        |context| app.update_at(context, frame_at + Duration::from_millis(5)),
    );
    let next_points = workload_series_points(&next_output);

    // Assert: Existing samples move continuously while the newest-value extension stays at the edge.
    let five_ms_shift = plot_width * (0.005 / 3.0) as f32;
    assert!((points[0].x - next_points[0].x - five_ms_shift).abs() < 0.1);
    assert!((points[5].x - next_points[5].x).abs() < 0.1);

    // Act: Render before sample receipt and well after the smoothing allowance.
    let early_output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
            ..Default::default()
        },
        |context| app.update_at(context, frame_at - Duration::from_millis(5)),
    );
    let early_points = workload_series_points(&early_output);
    let delayed_output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
            ..Default::default()
        },
        |context| app.update_at(context, frame_at + Duration::from_millis(100)),
    );
    let delayed_points = workload_series_points(&delayed_output);

    // Assert: The graph never runs ahead of a sample and stops scrolling at the 50 ms cap.
    assert!((early_points[0].x - points[0].x).abs() < 0.1);
    let capped_shift = plot_width * (0.05 / 3.0) as f32;
    assert!((points[0].x - delayed_points[0].x - capped_shift).abs() < 0.1);
    assert!((delayed_points[5].x - points[5].x).abs() < 0.1);
}

// What: Keep limit-sized gaps connected, break long gaps, and retain isolated measurements.
// Why: The graph must neither invent values during stalls nor hide observed samples.
#[test]
fn workload_graph_breaks_series_across_long_sample_gaps() {
    // Arrange: Prepare a connected pair, a lone sample, and a final pair separated by long gaps.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    let initial = inline.snapshot().unwrap().unwrap();
    let graph_snapshot = PresentedSnapshot {
        workload_samples: vec![
            WorkloadSample {
                elapsed_seconds: 1.0,
                workload_percent: 50.0,
            },
            WorkloadSample {
                elapsed_seconds: 1.05,
                workload_percent: 60.0,
            },
            WorkloadSample {
                elapsed_seconds: 1.101,
                workload_percent: 70.0,
            },
            WorkloadSample {
                elapsed_seconds: 1.2,
                workload_percent: 75.0,
            },
            WorkloadSample {
                elapsed_seconds: 1.24,
                workload_percent: 80.0,
            },
        ]
        .into(),
        ..initial
    };
    let backend = SnapshotBackend {
        snapshots: VecDeque::from([graph_snapshot]),
    };
    let mut app = Tek4051App::with_backend(&creation, backend).unwrap();
    let frame_at = Instant::now();

    // Act: Render and collect the graph paths, isolated sample marker, and second ticks.
    let output = context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
            ..Default::default()
        },
        |context| app.update_at(context, frame_at),
    );
    let mut paths: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Path(path)
                if path.stroke.width == 1.5
                    && matches!(
                        &path.stroke.color,
                        egui::epaint::ColorMode::Solid(color)
                            if *color == egui::Color32::LIGHT_BLUE
                    ) =>
            {
                Some(path.points.to_vec())
            }
            _ => None,
        })
        .collect();
    paths.sort_by_key(Vec::len);
    let lone_markers: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Circle(circle) if circle.fill == egui::Color32::LIGHT_BLUE => {
                Some(circle.center)
            }
            _ => None,
        })
        .collect();
    let mut ticks: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::LineSegment { points, stroke }
                if stroke.color == egui::Color32::GRAY
                    && (points[0].x - points[1].x).abs() < 0.1
                    && ((points[1].y - points[0].y) - 8.0).abs() < 0.1 =>
            {
                Some(points[0].x)
            }
            _ => None,
        })
        .collect();
    ticks.sort_by(f32::total_cmp);

    // Assert: The 50 ms pair stays connected, larger gaps split, and the lone value remains plotted.
    assert_eq!(paths.iter().map(Vec::len).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(lone_markers.len(), 1);
    let plot_width = (ticks[1] - ticks[0]) * 3.0;
    let expected_lone_x = ticks[2] + plot_width * ((1.101_f64 - 1.0) / 3.0) as f32;
    assert!((lone_markers[0].x - expected_lone_x).abs() < 0.1);
    assert!(paths[0][1].x < lone_markers[0].x);
    assert!(lone_markers[0].x < paths[1][0].x);
}

// What: Choose x100 directly from the CPU-speed selector.
// Why: Switching between interactive and fast-program speeds should not require cycling every option.
#[test]
fn cpu_speed_selector_applies_the_selected_factor() {
    // Arrange: Start the app at x1 and locate the visible speed selector.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut app =
        Tek4051App::with_backend(&creation, InlineBackend::new(Machine::default())).unwrap();
    let now = Instant::now();
    let initial = frame(&mut app, &context, now, vec![]);
    let speed = initial
        .iter()
        .find_map(|(text, position)| (text == "x1").then_some(*position))
        .expect("x1 selector should be visible");

    // Act: Open the selector, then choose x100 from its listed options.
    for pressed in [true, false] {
        frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(speed),
                Event::PointerButton {
                    pos: speed,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let options = frame(&mut app, &context, now, vec![]);
    let speed_100 = options
        .iter()
        .find_map(|(text, position)| (text == "x100").then_some(*position))
        .expect("x100 option should be visible after opening the selector");
    for pressed in [true, false] {
        frame(
            &mut app,
            &context,
            now,
            vec![
                Event::PointerMoved(speed_100),
                Event::PointerButton {
                    pos: speed_100,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let updated = frame(&mut app, &context, now, vec![]);

    // Assert: The command updates the selected speed without cycling through intermediate factors.
    assert_eq!(app.snapshot.speed_index, SPEED_FACTORS.len() - 1);
    assert!(updated.iter().any(|(text, _)| text == "x100"));
}

// What: Render a host keyboard queue overflow as a visible emulation error.
// Why: A rejected input frame must be reported instead of silently losing key transitions.
#[test]
fn host_input_overflow_is_visible_to_the_user() {
    // Arrange: Start the app with a backend that rejects a frame at the queue limit.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    let backend = FailingBackend {
        initial: inline.snapshot().unwrap(),
        error: "host keyboard queue full (1024 pending key transitions); newest input frame was not queued"
            .to_owned(),
    };
    let mut app = Tek4051App::with_backend(&creation, backend).unwrap();
    app.send(Command::Input(Box::new(
        tek4051_emulator::emulation_input::InputFrame {
            actions: vec![InputAction::SequencedManualPress {
                key: tek4051_emulator::emulation_input::ManualKeyId(1),
                code: b'A',
                invert_shift: false,
                modifiers: tek4051_emulator::emulation_input::KeyModifiers::NONE,
            }],
            modifiers: tek4051_emulator::emulation_input::KeyModifiers::NONE,
            focused: true,
            reset_input: false,
        },
    )));

    // Act: Submit a command and paint twice so the top panel lays out the new error row.
    frame(&mut app, &context, Instant::now(), vec![]);
    let painted = frame(&mut app, &context, Instant::now(), vec![]);

    // Assert: The overflow text is retained and displayed in the emulation-status area.
    assert_eq!(
        app.worker_error.as_deref(),
        Some(
            "host keyboard queue full (1024 pending key transitions); newest input frame was not queued"
        )
    );
    assert!(app.backend.is_connected());
    assert!(
        painted
            .iter()
            .any(|(text, _)| text == "Emulation error: host keyboard queue full (1024 pending key transitions); newest input frame was not queued"),
        "painted text: {:?}",
        painted.iter().map(|(text, _)| text).collect::<Vec<_>>()
    );
}

// What: Coalesce idle input frames without dropping keyboard press or release edges.
// Why: Redundant frames consume bounded worker-queue slots while GPU rendering is busy.
#[test]
fn idle_repaints_do_not_enqueue_duplicate_input_frames() {
    // Arrange: Use a pending backend so every submitted worker command remains observable.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    let app_backend = PendingBackend {
        initial: inline.snapshot().unwrap(),
        commands: Vec::new(),
    };
    let mut app = Tek4051App::with_backend(&creation, app_backend).unwrap();
    let now = Instant::now();
    frame(&mut app, &context, now, vec![]);
    let first_frame_count = app
        .backend
        .commands
        .iter()
        .filter(|command| matches!(command, Command::Input(_)))
        .count();

    // Act: Repaint repeatedly, then deliver an actual key press and release.
    for _ in 0..8 {
        frame(&mut app, &context, now, vec![]);
    }
    let idle_frame_count = app
        .backend
        .commands
        .iter()
        .filter(|command| matches!(command, Command::Input(_)))
        .count();
    let key = |pressed| Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&mut app, &context, now, vec![key(true)]);
    frame(&mut app, &context, now, vec![key(false)]);

    // Assert: Idle repaint count is stable, while both key edges remain queued.
    let input_frames: Vec<_> = app
        .backend
        .commands
        .iter()
        .filter_map(|command| match command {
            Command::Input(input) => Some(input),
            _ => None,
        })
        .collect();
    assert_eq!(idle_frame_count, first_frame_count);
    assert_eq!(input_frames.len(), first_frame_count + 2);
    let press = input_frames[input_frames.len() - 2]
        .actions
        .iter()
        .find_map(|action| match action {
            InputAction::SequencedManualPress { key, code, .. } => Some((*key, *code)),
            _ => None,
        })
        .expect("key press must remain queued");
    let release = input_frames[input_frames.len() - 1]
        .actions
        .iter()
        .find_map(|action| match action {
            InputAction::SequencedManualRelease(key) => Some(*key),
            _ => None,
        })
        .expect("matching key release must remain queued");
    assert_eq!(press, (release, b'A'));
}

// What: Route releases to the last worker-accepted key across ignored modal input.
// Why: GUI-side translation must not predict ownership from presses the modal worker discards.
#[test]
fn ignored_modal_presses_do_not_steal_manual_release_identity() {
    // Arrange: Send A through the real GUI translation and inline production command handler.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.write_byte(0x87a9, 0x04).unwrap();
    machine.write_byte(0x87ab, 0x04).unwrap();
    machine.keyboard_release();
    let mut app = Tek4051App::with_backend(&creation, InlineBackend::new(machine)).unwrap();
    app.send(Command::Run(true));
    observe_cursor_glyph(&mut app.backend.state);
    let key = |key, pressed| Event::Key {
        key,
        pressed,
        physical_key: None,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let now = Instant::now();
    frame(&mut app, &context, now, vec![key(egui::Key::A, true)]);
    assert_eq!(
        app.backend.state.data.machine.read_byte(0x87a8).unwrap(),
        0xc1
    );

    // Act: Translate B during modal suspension, then release B after input becomes accepted again.
    app.send(Command::Modal(true));
    frame(&mut app, &context, now, vec![key(egui::Key::B, true)]);
    app.send(Command::Modal(false));
    frame(&mut app, &context, now, vec![key(egui::Key::B, false)]);

    // Assert: Discarded B never replaces A or releases its accepted machine state.
    assert_eq!(
        app.backend.state.data.machine.read_byte(0x87a8).unwrap(),
        0xc1
    );

    // Act: Release the originally accepted A through the same GUI adapter.
    frame(&mut app, &context, now, vec![key(egui::Key::A, false)]);
    frame(&mut app, &context, now + Duration::from_millis(100), vec![]);

    // Assert: Its stable translated identity still reaches and releases the worker-owned key.
    assert_eq!(
        app.backend.state.data.machine.read_byte(0x87a8).unwrap(),
        0x80
    );
    assert!(app.backend.state.data.keyboard.is_idle());
}

// What: Operate native directory controls while no new emulation snapshot arrives.
// Why: Widget interaction must not depend on the CPU worker completing another tick.
#[test]
fn directory_remains_interactive_without_worker_progress() {
    // Arrange: Provide one initial state, then keep the backend pending for every later repaint.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut inline = InlineBackend::new(machine);
    let backend = PendingBackend {
        initial: inline.snapshot().unwrap(),
        commands: Vec::new(),
    };
    let mut app = Tek4051App::with_backend(&creation, backend).unwrap();
    let now = Instant::now();
    frame(&mut app, &context, now, vec![]);
    let positions = frame(&mut app, &context, now, vec![]);
    let position = positions
        .into_iter()
        .find_map(|(text, pos)| (text == "Show directory").then_some(pos))
        .unwrap();

    // Act: Click the directory checkbox using separate native press and release frames.
    for pressed in [true, false] {
        frame(
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
    let visible = frame(&mut app, &context, now, vec![]);

    // Assert: The panel opens using the retained snapshot, and input is still submitted asynchronously.
    assert!(app.show_flash_directory);
    assert!(
        visible
            .iter()
            .any(|(text, _)| text == "Flash Drive directory")
    );
    assert!(
        app.backend
            .commands
            .iter()
            .any(|command| matches!(command, Command::Input(_)))
    );
}

// What: Repaint while a file job is pending, then report that job's disconnection.
// Why: Slow or failed host I/O must neither block native controls nor disappear silently.
#[test]
fn pending_file_job_does_not_block_repaints_and_failure_is_visible() {
    // Arrange: Hold a screen-save job open while the worker retains its capture.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut inline = InlineBackend::new(machine);
    inline.command(Command::RequestScreenCopy).unwrap();
    let mut app = Tek4051App::with_backend(
        &creation,
        PendingBackend {
            initial: inline.snapshot().unwrap(),
            commands: Vec::new(),
        },
    )
    .unwrap();
    let capture_id = app.snapshot.screen_copy_requests[0].0;
    app.screen_in_flight = Some(capture_id);
    let (sender, receiver) = mpsc::channel();
    app.file_job = Some(receiver);
    let now = Instant::now();
    frame(&mut app, &context, now, vec![]);

    // Act: Render with no I/O result available, then disconnect the background producer.
    let pending = frame(&mut app, &context, now, vec![]);
    assert!(
        pending
            .iter()
            .any(|(text, _)| text == "File operation in progress...")
    );
    drop(sender);
    let failed = frame(&mut app, &context, now, vec![]);

    // Assert: The job failure is explicit and the worker is told to leave modal timing.
    assert!(
        failed
            .iter()
            .any(|(text, _)| { text == "file dialog worker ended without a result" })
    );
    assert!(app.file_job.is_none());
    assert!(
        app.backend
            .commands
            .iter()
            .any(|command| matches!(command, Command::Modal(false)))
    );
    assert!(
        app.backend.commands.iter().any(|command| {
            matches!(command, Command::CancelScreenCopy(id) if *id == capture_id)
        })
    );
    assert_eq!(app.screen_in_flight, Some(capture_id));
}

// What: Repaint and open the directory while the production worker runs at x100.
// Why: The real transport must keep native interaction independent of CPU catch-up.
#[test]
fn production_worker_keeps_gui_interactive_at_x100() {
    // Arrange: Start the real ROM worker and settle the native widget layout.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let initial_registers = machine.registers();
    let backend = WorkerBackend::start(machine).unwrap();
    let mut app = Tek4051App::with_backend(&creation, backend).unwrap();
    frame(&mut app, &context, Instant::now(), vec![]);
    frame(&mut app, &context, Instant::now(), vec![]);
    app.send(Command::Speed(SPEED_FACTORS.len() - 1));
    app.send(Command::Run(true));

    // Act: Wait for ROM progress and the overload notice, then open the directory and request Pause.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut longest_frame = Duration::ZERO;
    while Instant::now() < deadline {
        let started = Instant::now();
        frame(&mut app, &context, started, vec![]);
        longest_frame = longest_frame.max(started.elapsed());
        if app.snapshot.running
            && app.snapshot.registers != initial_registers
            && !app.snapshot.coalesced_time.is_zero()
        {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    let progressed = app.snapshot.running
        && app.snapshot.speed_index == SPEED_FACTORS.len() - 1
        && app.snapshot.registers != initial_registers;
    let positions = frame(&mut app, &context, Instant::now(), vec![]);
    let position = positions
        .into_iter()
        .find_map(|(text, pos)| (text == "Show directory").then_some(pos))
        .unwrap();
    for pressed in [true, false] {
        frame(
            &mut app,
            &context,
            Instant::now(),
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
    frame(&mut app, &context, Instant::now(), vec![]);
    let visible = frame(&mut app, &context, Instant::now(), vec![]);
    let pause = visible
        .iter()
        .find_map(|(text, pos)| (text == "Pause").then_some(*pos))
        .unwrap();
    for pressed in [true, false] {
        frame(
            &mut app,
            &context,
            Instant::now(),
            vec![
                Event::PointerMoved(pause),
                Event::PointerButton {
                    pos: pause,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.snapshot.running && Instant::now() < deadline {
        let started = Instant::now();
        frame(&mut app, &context, started, vec![]);
        longest_frame = longest_frame.max(started.elapsed());
        thread::sleep(Duration::from_millis(1));
    }
    let paused = !app.snapshot.running && app.snapshot.runtime_error.is_none();
    app.backend.join().unwrap();

    // Assert: CPU progress, directory interaction, Pause delivery and bounded repaints all succeed.
    assert!(progressed, "worker did not execute ROM instructions");
    assert!(paused, "native Pause did not reach the running worker");
    assert!(app.show_flash_directory);
    assert!(
        visible
            .iter()
            .any(|(text, _)| text == "Flash Drive directory")
    );
    assert!(
        longest_frame < Duration::from_millis(250),
        "repaint took {longest_frame:?}"
    );
    eprintln!("x100 worker: longest observed native widget frame {longest_frame:?}");
}

// What: Wait for the worker's modal acknowledgement before using export metadata.
// Why: A running program may replace a file after the last painted directory snapshot.
#[test]
fn export_waits_for_matching_frozen_store_snapshot() {
    // Arrange: Show an exportable file, then request an asynchronously acknowledged export.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    inline
        .command(Command::ImportFiles(vec![FlashDriveFile {
            name: "7      ASCII   PROG Original 999".to_owned(),
            bytes: b"old\r".to_vec(),
        }]))
        .unwrap();
    let mut app = Tek4051App::with_backend(
        &creation,
        PendingBackend {
            initial: inline.snapshot().unwrap(),
            commands: Vec::new(),
        },
    )
    .unwrap();
    app.queue_file_job(FileRequest::Export(7));

    // Act: A busy snapshot from an older operation must not open the chooser.
    inline.command(Command::BeginFileOperation(0)).unwrap();
    app.snapshot = inline.snapshot().unwrap().unwrap().snapshot;
    app.open_pending_file_job();
    assert!(app.pending_file.is_some());
    assert!(app.file_job.is_none());

    // Act: Acknowledge this request after the file acquires an unsafe export name.
    inline
        .command(Command::ImportFiles(vec![FlashDriveFile {
            name: "7      ASCII   PROG ../changed 999".to_owned(),
            bytes: b"new\r".to_vec(),
        }]))
        .unwrap();
    inline.command(Command::BeginFileOperation(1)).unwrap();
    app.snapshot = inline.snapshot().unwrap().unwrap().snapshot;
    app.open_pending_file_job();

    // Assert: Current frozen metadata rejects export; no stale-name dialog or host write starts.
    assert!(app.pending_file.is_none());
    assert!(app.file_job.is_none());
    assert!(
        matches!(&app.file_notice, Some((true, message)) if message.contains("lossless host filename"))
    );
    assert!(
        app.backend
            .commands
            .iter()
            .any(|command| matches!(command, Command::Modal(false)))
    );
}

// What: Keep cursor readiness cleared until the worker acknowledges the reset request.
// Why: A stale ready snapshot must not override the GUI's pending reset transition.
#[test]
fn stale_snapshots_cannot_restore_cursor_readiness_during_reset() {
    // Arrange: A running worker has not yet processed the GUI's Reset and readiness-clear requests.
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut inline = InlineBackend::new(Machine::default());
    inline.command(Command::Run(true)).unwrap();
    let mut app = Tek4051App::with_backend(
        &creation,
        PendingBackend {
            initial: inline.snapshot().unwrap(),
            commands: Vec::new(),
        },
    )
    .unwrap();
    app.send(Command::Reset);
    let clear = app.prompt_command(false);
    app.send(clear);

    // Act: Deliver an old snapshot, then a stale-ready snapshot while Reset remains queued.
    app.backend.initial = inline.snapshot().unwrap();
    app.refresh();
    assert_eq!(app.prompt_request_pending, Some(1));
    let mut stale_ready = inline.snapshot().unwrap().unwrap();
    std::sync::Arc::make_mut(&mut stale_ready.snapshot).basic_prompt_ready = true;
    app.backend.initial = Some(stale_ready);
    app.refresh();

    // Assert: Stale readiness cannot overwrite the GUI's reset-driven clear.
    assert!(!app.basic_prompt_ready);
    assert_eq!(app.prompt_request_pending, Some(1));

    // Act: Deliver the actual reset acknowledgement.
    inline.command(Command::Reset).unwrap();
    inline
        .command(Command::PromptReady {
            ready: false,
            request: 1,
        })
        .unwrap();
    app.backend.initial = inline.snapshot().unwrap();
    app.refresh();

    // Assert: The matching request completes synchronization without restoring readiness.
    assert!(!app.basic_prompt_ready);
    assert!(app.prompt_request_pending.is_none());
}

// What: Repaint and operate controls after a real export starts writing to a gated sink.
// Why: Pending-dialog stubs alone cannot prove that host writes stay off the GUI thread.
#[cfg(unix)]
#[test]
fn blocked_host_export_keeps_gui_responsive_and_reports_completion() {
    // Arrange: Read one FIFO byte to prove export reached host I/O, then stop draining its large payload.
    let directory = tempfile::Builder::new()
        .prefix(".worker-io-")
        .tempdir_in(".")
        .unwrap();
    let fifo = directory.path().join("export.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let (release, waiting) = mpsc::channel::<()>();
    let (started_sender, started) = mpsc::channel();
    let (result_sender, results) = mpsc::channel();
    let reader = thread::spawn(move || {
        let result = (|| -> std::io::Result<Vec<u8>> {
            let mut file = std::fs::File::open(fifo)?;
            let mut bytes = vec![0];
            std::io::Read::read_exact(&mut file, &mut bytes)?;
            started_sender.send(()).map_err(std::io::Error::other)?;
            // Disconnection also releases the FIFO during assertion unwinding.
            let _ = waiting.recv();
            std::io::Read::read_to_end(&mut file, &mut bytes)?;
            Ok(bytes)
        })();
        let _ = result_sender.send(result);
    });
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut machine = Machine::default();
    machine.reset().unwrap();
    let mut app =
        Tek4051App::with_backend(&creation, WorkerBackend::start(machine).unwrap()).unwrap();
    let bytes = vec![0x5a; 8 * 1024 * 1024];
    app.send(Command::ImportFiles(vec![FlashDriveFile {
        name: "7      ASCII   PROG Test 999".to_owned(),
        bytes: bytes.to_vec(),
    }]));
    app.send(Command::Run(true));
    app.send(Command::ExportFile {
        number: 7,
        path: directory.path().join("export.fifo"),
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while (!app.snapshot.busy || !app.snapshot.running) && Instant::now() < deadline {
        frame(&mut app, &context, Instant::now(), vec![]);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        app.snapshot.busy && app.snapshot.running,
        "worker did not enter host export"
    );
    frame(&mut app, &context, Instant::now(), vec![]);
    let controls = frame(&mut app, &context, Instant::now(), vec![]);
    let positions = ["Show directory", "Pause"].map(|label| {
        controls
            .iter()
            .find_map(|(text, position)| (text == label).then_some(*position))
            .unwrap()
    });

    // Act: Open the directory and queue Pause while the export cannot complete.
    let mut longest_frame = Duration::ZERO;
    for position in positions {
        for pressed in [true, false] {
            let started = Instant::now();
            frame(
                &mut app,
                &context,
                started,
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
            longest_frame = longest_frame.max(started.elapsed());
        }
    }

    // Assert: Widget changes are immediate even though the worker cannot apply Pause yet.
    assert!(app.show_flash_directory);
    assert!(app.snapshot.running && app.snapshot.busy);
    assert!(
        longest_frame < Duration::from_millis(250),
        "blocked-I/O repaint took {longest_frame:?}"
    );

    // Act: Resume draining the FIFO, allowing the already-started write and queued Pause to finish.
    release.send(()).unwrap();
    let saved = results
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    reader.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while (app.snapshot.running || app.snapshot.busy || app.snapshot.export_notice.is_none())
        && Instant::now() < deadline
    {
        frame(&mut app, &context, Instant::now(), vec![]);
        thread::sleep(Duration::from_millis(1));
    }
    app.backend.join().unwrap();

    // Assert: The exported bytes and successful result survive the delayed write.
    assert_eq!(saved, bytes);
    assert!(!app.snapshot.running && !app.snapshot.busy);
    assert!(
        matches!(&app.snapshot.export_notice, Some((false, message)) if message.contains("exported"))
    );
    assert!(app.snapshot.runtime_error.is_none());
}

fn frame<B: Backend>(
    app: &mut Tek4051App<B>,
    context: &egui::Context,
    now: Instant,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
    context
        .run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 720.0))),
                events,
                ..Default::default()
            },
            |context| app.update_at(context, now),
        )
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            Shape::Text(text) => Some((text.galley.job.text.clone(), text.pos + Vec2::splat(2.0))),
            _ => None,
        })
        .collect()
}
