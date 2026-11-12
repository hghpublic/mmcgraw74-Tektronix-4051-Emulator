use crate::machine::display::{DisplayControl, DisplayOperation, ScreenCommand};

use super::{CpuReferenceRenderer, DisplayEvent, DisplayGranularity, DisplayPoint};

#[test]
fn transient_glow_vector_keeps_its_base_geometry_across_refresh() {
    // What: Refresh preserves a vector stroke while removing the transient cursor mark.
    // Why: The glow's refresh lifetime must not erase the underlying geometry.
    // Arrange: Draw a transient-glow vector, then place a separate refresh-only cursor mark.
    let mut renderer = CpuReferenceRenderer::default();
    renderer.set_granularity(DisplayGranularity::Fine);
    renderer.apply_operations([DisplayOperation::ScreenCommand {
        command: ScreenCommand::BufferClock,
        control: DisplayControl {
            x_character: 0,
            y_character: 0,
            vector_0: false,
            ven_1: false,
            x: 20,
            y: 20,
        },
    }]);
    renderer.apply_operations([DisplayOperation::ScreenCommand {
        command: ScreenCommand::StartOfText,
        control: DisplayControl {
            x_character: 5,
            y_character: 5,
            vector_0: true,
            ven_1: false,
            x: 0,
            y: 0,
        },
    }]);
    let vector_pixel = (36, CpuReferenceRenderer::HEIGHT - 31);
    let cursor_pixel = (46, CpuReferenceRenderer::HEIGHT - 41);
    assert_eq!(
        renderer.take_events().events,
        vec![
            DisplayEvent::TransientVector {
                from: DisplayPoint { x: 0, y: 0 },
                to: DisplayPoint { x: 36, y: 30 },
            },
            DisplayEvent::TransientCursorMark(DisplayPoint { x: 46, y: 40 }),
        ]
    );
    assert!(
        renderer
            .pixel_intensity(vector_pixel.0, vector_pixel.1)
            .unwrap()
            > 0
    );
    assert!(
        renderer
            .pixel_intensity(cursor_pixel.0, cursor_pixel.1)
            .unwrap()
            > 0
    );

    // Act: Apply the normal display refresh.
    renderer.refresh();

    // Assert: The vector remains at stored intensity, while the cursor mark is cleared.
    assert_eq!(
        renderer
            .pixel_intensity(vector_pixel.0, vector_pixel.1)
            .unwrap(),
        254
    );
    assert_eq!(
        renderer
            .pixel_intensity(cursor_pixel.0, cursor_pixel.1)
            .unwrap(),
        0
    );
}

#[test]
fn erase_completion_clears_old_vectors_but_preserves_new_vectors() {
    // What: Erase completion removes pre-erase geometry without dropping later strokes.
    // Why: BASIC can draw new vectors before the machine's delayed erase callback completes.
    // Arrange: Draw a stored-intensity vector that should be removed by the next erase.
    let mut renderer = CpuReferenceRenderer::default();
    renderer.set_granularity(DisplayGranularity::Fine);
    renderer.apply_operations([
        DisplayOperation::ScreenCommand {
            command: ScreenCommand::BufferClock,
            control: DisplayControl {
                x_character: 0,
                y_character: 0,
                vector_0: true,
                ven_1: false,
                x: 10,
                y: 10,
            },
        },
        DisplayOperation::ScreenCommand {
            command: ScreenCommand::BufferClock,
            control: DisplayControl {
                x_character: 0,
                y_character: 0,
                vector_0: false,
                ven_1: false,
                x: 20,
                y: 10,
            },
        },
    ]);
    let old_vector_pixel = (26, CpuReferenceRenderer::HEIGHT - 1 - 20);
    let new_vector_pixel = (26, CpuReferenceRenderer::HEIGHT - 1 - 30);
    assert_eq!(
        renderer.pixel_intensity(old_vector_pixel.0, old_vector_pixel.1),
        Some(254)
    );

    // Act: Start erase, draw a new vector during its active interval, then complete it.
    renderer.apply_operations([DisplayOperation::BeginErase]);
    renderer.apply_operations([
        DisplayOperation::ScreenCommand {
            command: ScreenCommand::BufferClock,
            control: DisplayControl {
                x_character: 0,
                y_character: 0,
                vector_0: true,
                ven_1: false,
                x: 10,
                y: 20,
            },
        },
        DisplayOperation::ScreenCommand {
            command: ScreenCommand::BufferClock,
            control: DisplayControl {
                x_character: 0,
                y_character: 0,
                vector_0: false,
                ven_1: false,
                x: 20,
                y: 20,
            },
        },
        DisplayOperation::CompleteErase,
    ]);

    // Assert: Completion clears the old vector and reveals the stroke drawn during erase.
    assert_eq!(
        renderer.pixel_intensity(old_vector_pixel.0, old_vector_pixel.1),
        Some(0)
    );
    assert_eq!(
        renderer.pixel_intensity(new_vector_pixel.0, new_vector_pixel.1),
        Some(254)
    );
}
