use super::{
    DisplayGranularity, DisplayPoint, PendingDisplayGeometry, TransientGeometry, VectorSegment,
    WebGpuRendererError, build_segment_instances, checked_segment_instance_count,
    cursor_mark_segments, segment_instance_bytes, validate_dimensions,
};

#[cfg(not(target_arch = "wasm32"))]
use super::super::display::{DisplayEvent, DisplayEventBatch};
#[cfg(not(target_arch = "wasm32"))]
use super::{DeferredWebGpuSubmissions, MAX_PENDING_GPU_SUBMISSIONS, WebGpuVectorRenderer};
#[cfg(not(target_arch = "wasm32"))]
use crate::machine::display::{DisplayControl, DisplayOperation, ScreenCommand};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

// What: Rejects either empty axis and any axis beyond the supplied device limit.
// Why: Texture dimensions must be validated before the renderer allocates GPU resources.
#[test]
fn validates_offscreen_dimensions_at_each_boundary() {
    // Arrange: Use a representative device texture limit for dimension checks.
    let maximum = 8192;

    // Act: Validate zero axes, width/height limits, and the exact inclusive limit.
    let zero_width = validate_dimensions(0, 8, maximum);
    let zero_height = validate_dimensions(8, 0, maximum);
    let over_width = validate_dimensions(maximum + 1, 8, maximum);
    let over_height = validate_dimensions(8, maximum + 1, maximum);
    let at_limit = validate_dimensions(maximum, maximum, maximum);

    // Assert: Invalid targets report exact dimensions while the inclusive limit is accepted.
    assert_eq!(
        zero_width,
        Err(WebGpuRendererError::InvalidDimensions {
            width: 0,
            height: 8
        })
    );
    assert_eq!(
        zero_height,
        Err(WebGpuRendererError::InvalidDimensions {
            width: 8,
            height: 0
        })
    );
    assert_eq!(
        over_width,
        Err(WebGpuRendererError::DimensionsExceedDevice {
            width: maximum + 1,
            height: 8,
            maximum
        })
    );
    assert_eq!(
        over_height,
        Err(WebGpuRendererError::DimensionsExceedDevice {
            width: 8,
            height: maximum + 1,
            maximum
        })
    );
    assert_eq!(at_limit, Ok(()));
}

// What: Rejects malformed segment values and retains the segment's batch index.
// Why: Invalid geometry must fail before vertex upload or GPU submission.
#[test]
fn rejects_invalid_segments_with_their_batch_index() {
    // Arrange: Pair one valid segment with invalid values, widths, and finite-overflow geometry.
    let valid = VectorSegment::new([0.0, 0.0], [4.0, 4.0], 1.0, 0.5);
    let invalid_segments = [
        VectorSegment::new([f32::NAN, 0.0], [4.0, 4.0], 1.0, 0.5),
        VectorSegment::new([0.0, 0.0], [4.0, 4.0], 0.0, 0.5),
        VectorSegment::new([0.0, 0.0], [4.0, 4.0], -1.0, 0.5),
        VectorSegment::new([0.0, 0.0], [4.0, 4.0], 1.0, -0.01),
        VectorSegment::new([0.0, 0.0], [4.0, 4.0], 1.0, 1.01),
        VectorSegment::new([f32::MAX, 0.0], [-f32::MAX, 0.0], 1.0, 0.5),
        VectorSegment::new([0.0, 0.0], [f32::MAX, f32::MAX], 1.0, 0.5),
        VectorSegment::new([0.0, 0.0], [f32::MAX, 0.0], f32::MAX, 0.5),
    ];

    // Act: Build each batch with the invalid segment following a valid entry.
    let errors: Vec<_> = invalid_segments
        .into_iter()
        .map(|invalid| {
            build_segment_instances(&[valid, invalid])
                .err()
                .expect("invalid segment should be rejected")
        })
        .collect();

    // Assert: Invalid values, overflow, and non-positive widths are rejected at index one.
    assert!(
        errors
            .iter()
            .all(|error| *error == WebGpuRendererError::InvalidSegment { index: 1 })
    );
}

// What: Accepts the inclusive intensity endpoints of the public vector contract.
// Why: Full darkness and full green are valid inputs and must not be excluded by validation.
#[test]
fn accepts_zero_and_full_intensity_segments() {
    // Arrange: Use otherwise-valid vectors at both normalized intensity limits.
    let segments = [
        VectorSegment::new([0.0, 0.0], [1.0, 1.0], 1.0, 0.0),
        VectorSegment::new([1.0, 1.0], [2.0, 2.0], 1.0, 1.0),
    ];

    // Act: Prepare one compact GPU instance for each public segment.
    let instances =
        build_segment_instances(&segments).expect("inclusive intensities should be valid");

    // Assert: Both inclusive endpoints remain valid and produce drawable instances.
    assert_eq!(instances.len(), 2);
}

// What: Computes normalized directions and a finite fallback for zero-length segments.
// Why: The GPU expansion must retain the CPU geometry's established orientation rules.
#[test]
fn derives_segment_directions_and_zero_length_fallback() {
    // Arrange: Use a 3-4-5 vector and a point segment with otherwise valid rendering inputs.
    let segments = [
        VectorSegment::new([2.0, 4.0], [5.0, 8.0], 1.0, 0.5),
        VectorSegment::new([7.0, 9.0], [7.0, 9.0], 1.0, 0.5),
    ];

    // Act: Build the compact segment instances consumed by the GPU vertex shader.
    let instances =
        build_segment_instances(&segments).expect("both segment directions should be valid");

    // Assert: The nonzero vector is normalized and the point uses the horizontal fallback.
    assert!((instances[0].direction[0] - 0.6).abs() < 0.000001);
    assert!((instances[0].direction[1] - 0.8).abs() < 0.000001);
    assert_eq!(instances[1].direction, [1.0, 0.0]);
}

// What: Appends new transient geometry in the same packed order as a full rebuild.
// Why: Cached display redraws must preserve every segment's validated GPU instance data.
#[test]
fn appends_transient_bytes_without_rewriting_existing_instances() {
    // Arrange: Seed the cache with one segment and prepare two later segments in order.
    let first = VectorSegment::new([2.0, 4.0], [5.0, 8.0], 1.0, 0.5);
    let additions = [
        VectorSegment::new([7.0, 9.0], [11.0, 9.0], 2.0, 0.25),
        VectorSegment::new([12.0, 13.0], [12.0, 17.0], 1.0, 1.0),
    ];
    let mut cached = TransientGeometry::default();
    cached
        .append(&[first])
        .expect("the initial segment should be valid");
    let original = cached.instance_bytes.clone();
    let expected_segments = [first, additions[0], additions[1]];
    let expected = segment_instance_bytes(
        &build_segment_instances(&expected_segments).expect("all segments should be valid"),
    );

    // Act: Append the later geometry after the existing cached instance.
    cached
        .append(&additions)
        .expect("valid transient geometry should be cached");

    // Assert: The packed payload matches a full ordered build and retains its prefix.
    assert_eq!(cached.segment_count, expected_segments.len());
    assert_eq!(cached.instance_bytes, expected);
    assert_eq!(
        &cached.instance_bytes[..original.len()],
        original.as_slice()
    );
}

// What: Coalesces identical cursor marks until transient geometry refreshes.
// Why: Repeated ROM cursor marks have max-blended output and need not be packed or redrawn again.
#[test]
fn deduplicates_cursor_marks_until_refresh_by_visual_intensity() {
    // Arrange: Prepare a cursor position, a moved position, and empty transient state.
    let position = DisplayPoint { x: 16, y: 16 };
    let moved_position = DisplayPoint { x: 18, y: 16 };
    let mut cached = TransientGeometry::default();
    let mut stored_segments = Vec::new();
    let mut erase_flash = false;
    let mut display_dirty = false;
    let mut expected_segments = Vec::new();

    // Act: Stage the same mark twice at one intensity and once at a different intensity.
    let mut first_batch = PendingDisplayGeometry::default();
    first_batch.stage_cursor_mark(position, DisplayGranularity::Fine, 64, &cached);
    first_batch.stage_cursor_mark(position, DisplayGranularity::Fine, 64, &cached);
    first_batch.stage_cursor_mark(position, DisplayGranularity::Coarse, 64, &cached);
    expected_segments.extend(cursor_mark_segments(position, DisplayGranularity::Fine, 64));
    expected_segments.extend(cursor_mark_segments(
        position,
        DisplayGranularity::Coarse,
        64,
    ));
    first_batch
        .commit(
            &mut cached,
            &mut stored_segments,
            &mut erase_flash,
            &mut display_dirty,
        )
        .expect("distinct cursor appearances should append");

    // Assert: Exact duplicates vanish, but the changed brightness remains drawable.
    let expected = segment_instance_bytes(
        &build_segment_instances(&expected_segments).expect("staged marks should be valid"),
    );
    assert_eq!(cached.segment_count, 4);
    assert_eq!(cached.instance_bytes, expected);
    assert!(display_dirty);

    // Act: Repeat both cached appearances without adding any new geometry.
    display_dirty = false;
    let mut duplicate_batch = PendingDisplayGeometry::default();
    duplicate_batch.stage_cursor_mark(position, DisplayGranularity::Fine, 64, &cached);
    duplicate_batch.stage_cursor_mark(position, DisplayGranularity::Coarse, 64, &cached);
    duplicate_batch
        .commit(
            &mut cached,
            &mut stored_segments,
            &mut erase_flash,
            &mut display_dirty,
        )
        .expect("duplicate cursor marks should be ignored");

    // Assert: Duplicate-only batches leave geometry untouched and do not request a GPU redraw.
    assert_eq!(cached.segment_count, 4);
    assert_eq!(cached.instance_bytes, expected);
    assert!(!display_dirty);

    // Act: Add a new cursor position during the same transient refresh interval.
    let mut moved_batch = PendingDisplayGeometry::default();
    moved_batch.stage_cursor_mark(moved_position, DisplayGranularity::Fine, 64, &cached);
    expected_segments.extend(cursor_mark_segments(
        moved_position,
        DisplayGranularity::Fine,
        64,
    ));
    moved_batch
        .commit(
            &mut cached,
            &mut stored_segments,
            &mut erase_flash,
            &mut display_dirty,
        )
        .expect("a new cursor position should append");

    // Assert: The moved mark is retained without re-encoding existing cursor positions.
    let expected = segment_instance_bytes(
        &build_segment_instances(&expected_segments).expect("all staged marks should be valid"),
    );
    assert_eq!(cached.segment_count, 6);
    assert_eq!(cached.instance_bytes, expected);
    assert!(display_dirty);

    // Act: Clear transient state at refresh, then stage the original mark again.
    let did_refresh = cached.refresh();
    display_dirty = false;
    let mut refreshed_batch = PendingDisplayGeometry::default();
    refreshed_batch.stage_cursor_mark(position, DisplayGranularity::Fine, 64, &cached);
    refreshed_batch
        .commit(
            &mut cached,
            &mut stored_segments,
            &mut erase_flash,
            &mut display_dirty,
        )
        .expect("the cursor mark should be accepted after refresh");

    // Assert: Refresh clears both geometry and deduplication state for the next blink.
    let refreshed_segments = cursor_mark_segments(position, DisplayGranularity::Fine, 64);
    let expected = segment_instance_bytes(
        &build_segment_instances(&refreshed_segments).expect("refreshed mark should be valid"),
    );
    assert!(did_refresh);
    assert_eq!(cached.segment_count, 2);
    assert_eq!(cached.instance_bytes, expected);
    assert!(display_dirty);
}

// What: Rolls back a transient append when a later segment fails validation.
// Why: A rejected update must not leave partial geometry or cursor deduplication state in the cache.
#[test]
fn rolls_back_transient_bytes_after_invalid_geometry() {
    // Arrange: Keep existing renderer state and stage stored/cursor updates around invalid geometry.
    let initial = VectorSegment::new([1.0, 2.0], [4.0, 6.0], 1.0, 0.5);
    let additions = [
        VectorSegment::new([4.0, 6.0], [7.0, 8.0], 1.0, 0.5),
        VectorSegment::new([0.0, 0.0], [1.0, 1.0], 0.0, 0.5),
    ];
    let mut cached = TransientGeometry::default();
    cached
        .append(&[initial])
        .expect("the initial segment should be valid");
    let original_count = cached.segment_count;
    let original_bytes = cached.instance_bytes.clone();
    let existing_stored = VectorSegment::new([2.0, 3.0], [5.0, 6.0], 1.0, 0.5);
    let new_stored = VectorSegment::new([7.0, 8.0], [9.0, 10.0], 1.0, 0.5);
    let mut stored_segments = vec![existing_stored];
    let mut erase_flash = false;
    let mut display_dirty = false;

    // Act: Commit a display update whose transient append fails after a valid segment.
    let mut pending = PendingDisplayGeometry {
        stored_segments: vec![new_stored],
        transient_segments: additions.to_vec(),
        transient_cursor_mark_keys: Vec::new(),
        erase_flash: true,
        has_events: true,
    };
    pending.stage_cursor_mark(
        DisplayPoint { x: 16, y: 16 },
        DisplayGranularity::Fine,
        64,
        &cached,
    );
    let result = pending.commit(
        &mut cached,
        &mut stored_segments,
        &mut erase_flash,
        &mut display_dirty,
    );

    // Assert: Failure preserves the cache, stored geometry, and operation flags.
    assert_eq!(
        result,
        Err(WebGpuRendererError::InvalidSegment { index: 2 })
    );
    assert_eq!(cached.segment_count, original_count);
    assert_eq!(cached.instance_bytes, original_bytes);
    assert!(cached.cursor_mark_keys.is_empty());
    assert_eq!(stored_segments, [existing_stored]);
    assert!(!erase_flash);
    assert!(!display_dirty);
}

// What: Rejects a transient append beyond the GPU draw-count boundary.
// Why: The packed cache must not accept more segments than its renderer can draw.
#[test]
fn rejects_transient_append_beyond_gpu_instance_limit_without_mutation() {
    // Arrange: Model a full cache without allocating the multi-gigabyte instance payload.
    let maximum_count = u32::MAX as usize / 6;
    let mut cached = TransientGeometry {
        segment_count: maximum_count,
        instance_bytes: Vec::new(),
        cursor_mark_keys: std::collections::HashSet::new(),
    };
    let original_bytes = cached.instance_bytes.clone();
    let next_segment = VectorSegment::new([1.0, 2.0], [4.0, 6.0], 1.0, 0.5);

    // Act: Confirm the limit is accepted, then try to append one more segment.
    let limit_count = checked_segment_instance_count(maximum_count);
    let result = cached.append(&[next_segment]);

    // Assert: The next append fails explicitly and leaves the cache unchanged.
    assert_eq!(limit_count, Ok(maximum_count as u32));
    assert_eq!(result, Err(WebGpuRendererError::TooManyVertices));
    assert_eq!(cached.segment_count, maximum_count);
    assert_eq!(cached.instance_bytes, original_bytes);
}

// What: Replaces cached transient geometry at refresh and erase boundaries.
// Why: Old cursor geometry must not be redrawn after either transient-clear event.
#[test]
fn refresh_and_erase_clear_transient_geometry_before_the_next_batch() {
    // Arrange: Cache one segment and prepare distinct vectors for later display batches.
    let old = VectorSegment::new([1.0, 2.0], [4.0, 6.0], 1.0, 0.5);
    let after_refresh = VectorSegment::new([8.0, 10.0], [12.0, 10.0], 2.0, 0.25);
    let after_erase = VectorSegment::new([14.0, 15.0], [14.0, 19.0], 1.0, 1.0);
    let mut cached = TransientGeometry::default();
    cached
        .append(&[old])
        .expect("initial geometry should be valid");

    // Act: Refresh, append new geometry, erase it, then append one final new batch.
    let did_refresh = cached.refresh();
    cached
        .append(&[after_refresh])
        .expect("post-refresh geometry should be valid");
    let refreshed_count = cached.segment_count;
    let refreshed_bytes = cached.instance_bytes.clone();
    cached.clear();
    let erase_cleared_cache = cached.segment_count == 0 && cached.instance_bytes.is_empty();
    cached
        .append(&[after_erase])
        .expect("post-erase geometry should be valid");

    // Assert: Both boundaries discard old bytes and the next batch contains only new geometry.
    let expected_refresh_bytes = segment_instance_bytes(
        &build_segment_instances(&[after_refresh]).expect("post-refresh geometry should be valid"),
    );
    let expected_erase_bytes = segment_instance_bytes(
        &build_segment_instances(&[after_erase]).expect("post-erase geometry should be valid"),
    );
    assert!(did_refresh);
    assert_eq!(refreshed_count, 1);
    assert_eq!(refreshed_bytes, expected_refresh_bytes);
    assert!(erase_cleared_cache);
    assert_eq!(cached.segment_count, 1);
    assert_eq!(cached.instance_bytes, expected_erase_bytes);
}

#[cfg(not(target_arch = "wasm32"))]
fn native_gpu_context() -> Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>), String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|error| error.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Tektronix 4051 display batch test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    Ok((Arc::new(device), Arc::new(queue)))
}

// What: Preserves a staged display batch when the deferred queue is full, then retries it.
// Why: Queue saturation must report failure without losing renderer state or blocking forever.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "requires a native WebGPU adapter"]
fn deferred_queue_overflow_preserves_staged_batch_for_retry() {
    // Arrange: Fill the bounded queue and stage a visible vector behind the full queue.
    let (device, queue) = native_gpu_context().expect("native WebGPU adapter and device");
    let submissions = Arc::new(DeferredWebGpuSubmissions::new(Arc::clone(&queue)));
    let mut renderer = WebGpuVectorRenderer::new(Arc::clone(&device), Arc::clone(&queue), 32, 32)
        .expect("offscreen target should fit");
    renderer.set_deferred_submissions(Arc::clone(&submissions));
    let command_buffer = || {
        device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("deferred submission overflow test"),
            })
            .finish()
    };
    for _ in 0..MAX_PENDING_GPU_SUBMISSIONS {
        submissions
            .enqueue(command_buffer())
            .expect("queue should accept commands up to its capacity");
    }
    let control = DisplayControl {
        x_character: 0,
        y_character: 0,
        vector_0: false,
        ven_1: false,
        x: 0,
        y: 0,
    };
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::StoredVector {
                    from: DisplayPoint { x: 4, y: 4 },
                    to: DisplayPoint { x: 27, y: 25 },
                    size: 1,
                }],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("valid display operation should be staged");
    let initial_revision = renderer.revision();

    // Act: Hit the capacity error, drain the queue, retry the same batch, and submit it.
    let overflow = renderer.finish_display_batch();
    let revision_after_overflow = renderer.revision();
    submissions
        .submit_pending()
        .expect("the full queue should drain successfully");
    renderer
        .finish_display_batch()
        .expect("the staged vector should be retryable after draining");
    let revision_after_retry = renderer.revision();
    submissions
        .submit_pending()
        .expect("the retried batch should submit successfully");
    let pixels = renderer
        .readback_rgba()
        .expect("retried display frame should be readable");

    // Assert: Overflow is explicit, does not advance revision, and the retry renders the vector.
    assert_eq!(overflow, Err(WebGpuRendererError::PendingSubmissionsFull));
    assert_eq!(revision_after_overflow, initial_revision);
    assert_eq!(revision_after_retry, initial_revision.wrapping_add(1));
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[1] > 0));
}

// What: Keeps ordinary display batches pending until the native UI submits them.
// Why: Worker rendering must not contend with the GUI while it acquires a surface drawable.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "requires a native WebGPU adapter"]
fn deferred_display_batch_waits_for_ui_submission() {
    // Arrange: Connect a renderer to a deferred queue and stage a visible stored vector.
    let (device, queue) = native_gpu_context().expect("native WebGPU adapter and device");
    let submissions = Arc::new(DeferredWebGpuSubmissions::new(Arc::clone(&queue)));
    let mut renderer =
        WebGpuVectorRenderer::new(device, queue, 32, 32).expect("offscreen target should fit");
    renderer.set_deferred_submissions(Arc::clone(&submissions));
    let control = DisplayControl {
        x_character: 0,
        y_character: 0,
        vector_0: false,
        ven_1: false,
        x: 0,
        y: 0,
    };

    // Act: Finish the vector batch, observe before submission, then flush and read it again.
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::StoredVector {
                    from: DisplayPoint { x: 4, y: 4 },
                    to: DisplayPoint { x: 27, y: 25 },
                    size: 1,
                }],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("valid display operation should be staged");
    renderer
        .finish_display_batch()
        .expect("display batch should be deferred");
    let before_submission = renderer
        .readback_rgba()
        .expect("offscreen target should be readable before submission");
    submissions
        .submit_pending()
        .expect("native UI submission should succeed");
    let after_submission = renderer
        .readback_rgba()
        .expect("offscreen target should be readable after submission");

    // Assert: The vector is absent before the UI flush and visible after its queued commands run.
    assert!(before_submission.iter().all(|channel| *channel == 0));
    assert!(after_submission.chunks_exact(4).any(|pixel| pixel[1] > 0));
}

// What: Screen copies flush staged geometry, and erase preserves strokes drawn after it starts.
// Why: Machine captures must reflect the ordered display lifecycle and its delayed erase callback.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "requires a native WebGPU adapter"]
fn screen_copy_flushes_the_staged_display_batch() {
    // Arrange: Create a GPU renderer and exercise no-op and stored display events before capture.
    let (device, queue) = native_gpu_context().expect("native WebGPU adapter and device");
    let submissions = Arc::new(DeferredWebGpuSubmissions::new(Arc::clone(&queue)));
    let mut renderer =
        WebGpuVectorRenderer::new(device, queue, 32, 32).expect("offscreen target should fit");
    renderer.set_deferred_submissions(submissions);
    let initial_revision = renderer.revision();
    let control = DisplayControl {
        x_character: 0,
        y_character: 0,
        vector_0: false,
        ven_1: false,
        x: 0,
        y: 0,
    };
    let no_op_control = DisplayControl {
        vector_0: true,
        ..control
    };
    let stored_vector = DisplayEvent::StoredVector {
        from: DisplayPoint { x: 4, y: 4 },
        to: DisplayPoint { x: 27, y: 25 },
        size: 1,
    };

    // Act: Ignore an invisible AlphaDot, then stage a vector and flush it at the screen copy.
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::AlphaDot,
                control: no_op_control,
            },
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("invisible AlphaDot should be accepted");
    assert_eq!(renderer.revision(), initial_revision);
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![stored_vector],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("valid display operation should be staged");
    assert_eq!(renderer.revision(), initial_revision);
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCopy,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("screen copy should flush the staged vector");

    // Assert: The barrier renders both endpoints and leaves a distant pixel dark.
    // Its screen-copy snapshot matches the composed frame.
    assert_eq!(renderer.revision(), initial_revision.wrapping_add(1));
    let pixels = renderer
        .readback_rgba()
        .expect("composed frame should be readable");
    let green_at = |x: usize, y: usize| pixels[(y * 32 + x) * 4 + 1];
    assert!(green_at(4, 27) > 0);
    assert!(green_at(27, 6) > 0);
    assert_eq!(green_at(0, 0), 0);
    let (copies, dropped) = renderer.take_screen_copies();
    assert_eq!(dropped, 0);
    assert_eq!(copies.len(), 1);
    assert_eq!(
        copies[0],
        pixels
            .chunks_exact(4)
            .map(|pixel| pixel[1])
            .collect::<Vec<_>>()
    );

    // Act: Begin erase, draw a new stored vector and transient cursor, then complete and capture.
    renderer
        .apply_display_operation(
            DisplayOperation::BeginErase,
            DisplayEventBatch {
                events: vec![DisplayEvent::Erase],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("erase start should clear the earlier vector");
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::StoredVector {
                    from: DisplayPoint { x: 8, y: 4 },
                    to: DisplayPoint { x: 8, y: 27 },
                    size: 1,
                }],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("the new vector should be staged during erase");
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::TransientCursorMark(DisplayPoint {
                    x: 16,
                    y: 16,
                })],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("transient cursor mark should be staged");
    renderer
        .apply_display_operation(
            DisplayOperation::CompleteErase,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("complete erase should be staged");
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCopy,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("screen copy should flush the erase");

    // Assert: Erase removes the old diagonal and transient cursor but retains the new vector.
    let pixels = renderer
        .readback_rgba()
        .expect("erased frame should be readable");
    let green_at = |x: usize, y: usize| pixels[(y * 32 + x) * 4 + 1];
    assert_eq!(green_at(4, 27), 0);
    assert!(green_at(8, 16) > 0);
    let (copies, dropped) = renderer.take_screen_copies();
    assert_eq!(dropped, 0);
    assert_eq!(copies.len(), 1);
    assert!(copies[0][16 * 32 + 8] > 0);
}

// What: Removes transient cursor geometry on refresh without clearing stored vectors.
// Why: The renderer refresh path must discard stale transient pixels while retaining phosphor content.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "requires a native WebGPU adapter"]
fn refresh_clears_transient_cursor_but_preserves_persistent_vectors() {
    // Arrange: Render a stored vector and retain its frame as the persistent-layer baseline.
    let (device, queue) = native_gpu_context().expect("native WebGPU adapter and device");
    let mut renderer =
        WebGpuVectorRenderer::new(device, queue, 32, 32).expect("offscreen target should fit");
    let control = DisplayControl {
        x_character: 0,
        y_character: 0,
        vector_0: false,
        ven_1: false,
        x: 0,
        y: 0,
    };
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::StoredVector {
                    from: DisplayPoint { x: 4, y: 4 },
                    to: DisplayPoint { x: 27, y: 4 },
                    size: 1,
                }],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("stored vector should be staged");
    renderer
        .finish_display_batch()
        .expect("stored vector should be rendered");
    let persistent_frame = renderer
        .readback_rgba()
        .expect("persistent frame should be readable");

    // Act: Add a cursor mark, then refresh and render the updated display.
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCommand {
                command: ScreenCommand::BufferClock,
                control,
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::TransientCursorMark(DisplayPoint {
                    x: 16,
                    y: 16,
                })],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("transient cursor mark should be staged");
    renderer
        .finish_display_batch()
        .expect("cursor mark should be rendered");
    let frame_with_cursor = renderer
        .readback_rgba()
        .expect("cursor frame should be readable");
    renderer
        .refresh_display()
        .expect("transient display refresh should succeed");
    let refreshed_frame = renderer
        .readback_rgba()
        .expect("refreshed frame should be readable");

    // Assert: Refresh removes the cursor mark and leaves the stored vector unchanged.
    assert!(persistent_frame.iter().any(|channel| *channel > 0));
    assert_ne!(frame_with_cursor, persistent_frame);
    assert_eq!(refreshed_frame, persistent_frame);
}
