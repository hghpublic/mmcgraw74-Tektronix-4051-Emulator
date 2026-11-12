use crate::{
    emulation::{
        display::{DisplayEvent, DisplayEventBatch, DisplayGranularity, DisplayPoint},
        vector_scene::VectorScene,
    },
    machine::{
        display::DisplayControl,
        display::{DisplayOperation, ScreenCommand},
    },
};

fn batch(events: Vec<DisplayEvent>) -> DisplayEventBatch {
    DisplayEventBatch {
        events,
        dropped: false,
    }
}

fn stored_operation() -> DisplayOperation {
    DisplayOperation::ScreenCommand {
        command: ScreenCommand::BufferClock,
        control: DisplayControl {
            x_character: 0,
            y_character: 0,
            vector_0: false,
            ven_1: false,
            x: 0,
            y: 0,
        },
    }
}

#[test]
fn refresh_removes_cursor_marks_but_keeps_all_vector_geometry() {
    // What: Refresh drops the cursor mark but retains both stored and transient-vector events.
    // Why: Later vector rerendering depends on the scene keeping every drawn stroke.
    // Arrange: Record one stored mark, two vector strokes, and one refresh-only cursor mark.
    let mut scene = VectorScene::default();
    scene
        .apply_display_operation(
            stored_operation(),
            &batch(vec![
                DisplayEvent::StoredMark {
                    position: DisplayPoint { x: 10, y: 20 },
                    size: 2,
                },
                DisplayEvent::StoredVector {
                    from: DisplayPoint { x: 10, y: 20 },
                    to: DisplayPoint { x: 30, y: 40 },
                    size: 1,
                },
                DisplayEvent::TransientCursorMark(DisplayPoint { x: 30, y: 40 }),
                DisplayEvent::TransientVector {
                    from: DisplayPoint { x: 30, y: 40 },
                    to: DisplayPoint { x: 50, y: 60 },
                },
            ]),
            DisplayGranularity::Coarse,
        )
        .expect("complete display events should be recorded");

    // Act: Apply the display refresh that ends the cursor mark's visible lifetime.
    scene.refresh_display();

    // Assert: Both strokes remain in the retained scene, but the cursor mark is gone.
    assert_eq!(scene.retained_geometry_events().count(), 3);
    assert_eq!(scene.transient_events().count(), 1);
    assert!(
        scene
            .retained_geometry_events()
            .all(|recorded| recorded.granularity() == DisplayGranularity::Coarse)
    );
    assert_eq!(
        scene
            .events
            .iter()
            .map(|recorded| recorded.event)
            .collect::<Vec<_>>(),
        vec![
            DisplayEvent::StoredMark {
                position: DisplayPoint { x: 10, y: 20 },
                size: 2,
            },
            DisplayEvent::StoredVector {
                from: DisplayPoint { x: 10, y: 20 },
                to: DisplayPoint { x: 30, y: 40 },
                size: 1,
            },
            DisplayEvent::TransientVector {
                from: DisplayPoint { x: 30, y: 40 },
                to: DisplayPoint { x: 50, y: 60 },
            },
        ]
    );
}

#[test]
fn erase_start_clears_old_geometry_and_completion_keeps_new_geometry() {
    // What: Erase start clears old geometry, and completion retains strokes drawn during the flash.
    // Why: A delayed erase callback must not remove vectors emitted after the erase began.
    // Arrange: Begin with a stored mark and start the ordered erase sequence.
    let mut scene = VectorScene::default();
    scene
        .apply_display_operation(
            stored_operation(),
            &batch(vec![DisplayEvent::StoredMark {
                position: DisplayPoint { x: 12, y: 18 },
                size: 1,
            }]),
            DisplayGranularity::Fine,
        )
        .expect("stored mark should be recorded");
    scene
        .apply_display_operation(
            DisplayOperation::BeginErase,
            &batch(vec![DisplayEvent::Erase]),
            DisplayGranularity::Fine,
        )
        .expect("erase start should be recorded");

    // Act: Capture the empty erase frame, then draw a new vector before completing erase.
    let scene_before_copy = scene.clone();
    scene
        .apply_display_operation(
            DisplayOperation::ScreenCopy,
            &batch(Vec::new()),
            DisplayGranularity::Fine,
        )
        .expect("screen copy should leave scene state unchanged");
    assert_eq!(scene, scene_before_copy);
    let scene_during_erase = scene.clone();
    scene
        .apply_display_operation(
            stored_operation(),
            &batch(vec![DisplayEvent::TransientVector {
                from: DisplayPoint { x: 20, y: 30 },
                to: DisplayPoint { x: 40, y: 50 },
            }]),
            DisplayGranularity::Fine,
        )
        .expect("vectors drawn during erase should be recorded");
    scene
        .apply_display_operation(
            DisplayOperation::CompleteErase,
            &batch(Vec::new()),
            DisplayGranularity::Fine,
        )
        .expect("erase completion should remove the flash");

    // Assert: Old geometry is gone at erase start, while the later stroke survives completion.
    assert!(scene_during_erase.erase_flash());
    assert_eq!(scene_during_erase.retained_geometry_events().count(), 0);
    assert!(!scene.erase_flash());
    assert_eq!(scene.retained_geometry_events().count(), 1);
    assert_eq!(scene.transient_events().count(), 1);
}

#[test]
fn dropped_display_history_is_reported_instead_of_captured_as_complete() {
    // What: A dropped event batch is rejected by the vector-scene recorder.
    // Why: A partial scene must not be presented as complete vector zoom.
    // Arrange: Mark the renderer's input batch as incomplete.
    let mut scene = VectorScene::default();
    let dropped_batch = DisplayEventBatch {
        events: Vec::new(),
        dropped: true,
    };

    // Act: Attempt to record the incomplete operation history.
    let result =
        scene.apply_display_operation(stored_operation(), &dropped_batch, DisplayGranularity::Fine);

    // Assert: The scene reports the loss and retains no partial geometry.
    assert!(result.is_err());
    assert_eq!(scene.retained_geometry_events().count(), 0);
    assert_eq!(scene.transient_events().count(), 0);
}

#[test]
fn memory_accounting_tracks_reserved_scene_storage() {
    // What: Scene memory reports reserved event storage and its high-water mark.
    // Why: Initial test-scene memory must be measured before deciding a future bound.
    // Arrange: Record enough persistent vectors to grow the retained event buffer.
    let mut scene = VectorScene::default();
    let events = (0..32)
        .map(|index| DisplayEvent::StoredVector {
            from: DisplayPoint { x: index, y: 10 },
            to: DisplayPoint {
                x: index + 1,
                y: 20,
            },
            size: 1,
        })
        .collect();
    scene
        .apply_display_operation(stored_operation(), &batch(events), DisplayGranularity::Fine)
        .expect("all stored vectors should be captured");

    // Act: Read the recorder's memory and high-water measurements.
    let memory = scene.memory_usage();

    // Assert: Reserved storage covers every record and is retained as the measured peak.
    assert_eq!(memory.retained_geometry_events(), 32);
    assert!(memory.current_bytes() > 0);
    assert!(memory.peak_bytes() >= memory.current_bytes());
}
