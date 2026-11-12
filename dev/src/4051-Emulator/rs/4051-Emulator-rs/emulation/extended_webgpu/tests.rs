use std::{sync::Arc, time::Duration};

use super::{
    DecayCurve, DecayParameters, ExtendedWebGpuRenderer, ExtendedWebGpuRendererError, TICK_MASK,
    WebGpuVectorRenderer,
};
use crate::{
    emulation::display::{DisplayEvent, DisplayEventBatch, DisplayGranularity, DisplayPoint},
    machine::display::{DisplayControl, DisplayOperation, ScreenCommand},
};

const WIDTH: u32 = 32;
const HEIGHT: u32 = 32;

fn native_gpu_context() -> Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>), String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|error| error.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Extended WebGPU renderer test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    Ok((Arc::new(device), Arc::new(queue)))
}

fn test_renderer() -> Result<ExtendedWebGpuRenderer, ExtendedWebGpuRendererError> {
    let (device, queue) = native_gpu_context().map_err(ExtendedWebGpuRendererError::WebGpu)?;
    ExtendedWebGpuRenderer::new(device, queue, WIDTH, HEIGHT)
}

fn stored_mark() -> DisplayEventBatch {
    stored_mark_at(8, 8)
}

fn stored_mark_at(x: i32, y: i32) -> DisplayEventBatch {
    DisplayEventBatch {
        events: vec![DisplayEvent::StoredMark {
            position: DisplayPoint { x, y },
            size: 1,
        }],
        dropped: false,
    }
}

fn apply_mark(renderer: &mut ExtendedWebGpuRenderer) {
    apply_mark_with_granularity(renderer, DisplayGranularity::Fine);
}

fn apply_mark_with_granularity(
    renderer: &mut ExtendedWebGpuRenderer,
    granularity: DisplayGranularity,
) {
    apply_mark_batch(renderer, stored_mark(), granularity);
}

fn apply_mark_batch(
    renderer: &mut ExtendedWebGpuRenderer,
    events: DisplayEventBatch,
    granularity: DisplayGranularity,
) {
    renderer
        .apply_display_operation(
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
            },
            events,
            granularity,
        )
        .expect("mark operation should be accepted");
    renderer
        .finish_display_batch()
        .expect("mark should be stamped");
    renderer
        .compose_frame()
        .expect("the phosphor frame should compose");
}

fn normal_webgpu_mark_frame(granularity: DisplayGranularity) -> Vec<u8> {
    let (device, queue) = native_gpu_context().expect("native WebGPU should be available");
    let mut renderer = WebGpuVectorRenderer::new(device, queue, WIDTH, HEIGHT)
        .expect("the standard WebGPU renderer should initialize");
    renderer
        .apply_display_operation(
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
            },
            stored_mark(),
            granularity,
        )
        .expect("the standard renderer should accept the mark");
    renderer
        .finish_display_batch()
        .expect("the standard renderer should finish the mark");
    renderer
        .readback_rgba()
        .expect("the standard renderer frame should be readable")
}

fn normal_webgpu_mark_intensity(granularity: DisplayGranularity) -> u8 {
    let frame = normal_webgpu_mark_frame(granularity);
    frame[(23 * WIDTH as usize + 8) * 4 + 1]
}

fn readback_rgba(renderer: &ExtendedWebGpuRenderer) -> Vec<u8> {
    readback_texture_rgba(renderer, &renderer._output_texture)
}

fn readback_vector_rgba(renderer: &ExtendedWebGpuRenderer) -> Vec<u8> {
    let vector_texture = &renderer
        .vector_zoom
        .as_ref()
        .expect("vector presentation should be allocated")
        ._output_texture;
    readback_texture_rgba(renderer, vector_texture)
}

fn readback_texture_rgba(renderer: &ExtendedWebGpuRenderer, texture: &wgpu::Texture) -> Vec<u8> {
    let padded_row_bytes = (WIDTH * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Extended WebGPU RGBA test readback"),
        size: u64::from(padded_row_bytes) * u64::from(HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Extended WebGPU RGBA test readback"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    renderer
        .device
        .poll(wgpu::PollType::Wait)
        .expect("the GPU should finish the test readback");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the test readback should complete")
        .expect("the test readback should map");

    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in mapped
        .chunks_exact(padded_row_bytes as usize)
        .take(HEIGHT as usize)
    {
        pixels.extend_from_slice(&row[..(WIDTH * 4) as usize]);
    }
    drop(mapped);
    buffer.unmap();
    pixels
}

fn center_intensity(renderer: &ExtendedWebGpuRenderer) -> u8 {
    pixel_intensity(renderer, 8, 23)
}

fn pixel_intensity(renderer: &ExtendedWebGpuRenderer, x: u32, y: u32) -> u8 {
    let pixels = renderer
        .readback_green()
        .expect("the GPU frame should be readable");
    pixels[y as usize * WIDTH as usize + x as usize]
}

fn pixel_intensity_from_rgba(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[(y as usize * WIDTH as usize + x as usize) * 4 + 1]
}

#[test]
fn vector_zoom_rerasterizes_scene_without_changing_screen_copy_pixels() {
    // What: Vector presentation moves retained geometry while the machine copy stays raster-based.
    // Why: Camera changes belong to presentation and must not alter machine ScreenCopy pixels.
    // Arrange: Capture one stored mark and retain the original camera-independent output frame.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer.set_vector_scene_capture_enabled(true);
    apply_mark_batch(
        &mut renderer,
        stored_mark_at(10, 10),
        DisplayGranularity::Fine,
    );
    apply_mark_batch(
        &mut renderer,
        stored_mark_at(20, 20),
        DisplayGranularity::Fine,
    );
    let original_frame = readback_rgba(&renderer);

    // Act: Enable vector mode at 2× and request a machine ScreenCopy.
    renderer
        .set_vector_zoom_camera(2.0, 0.0, 0.0)
        .expect("a finite supported camera should be accepted");
    renderer
        .set_vector_zoom_mode(true)
        .expect("captured geometry should enable vector presentation");
    let vector_frame = readback_vector_rgba(&renderer);
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCopy,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("ScreenCopy should remain available in vector mode");
    let (screen_copies, dropped) = renderer.take_screen_copies();

    // Assert: The vector mark is rerasterized at its transformed location, but copies retain the original frame.
    assert!(pixel_intensity_from_rgba(&vector_frame, 5, 27) > 0);
    assert!(pixel_intensity_from_rgba(&vector_frame, 25, 7) > 0);
    assert_eq!(pixel_intensity_from_rgba(&vector_frame, 10, 21), 0);
    assert_eq!(readback_rgba(&renderer), original_frame);
    assert_eq!(dropped, 0);
    assert_eq!(screen_copies.len(), 1);
    let expected_screen_copy = original_frame
        .chunks_exact(4)
        .map(|rgba| rgba[1])
        .collect::<Vec<_>>();
    assert_eq!(screen_copies[0], expected_screen_copy);
}

#[test]
fn vector_camera_accepts_zoom_limits_and_rejects_invalid_transforms() {
    // What: The vector camera accepts both supported zoom endpoints and rejects invalid transforms.
    // Why: Browser camera inputs must stay finite and within the established 0.25×–8× range.
    // Arrange: Capture geometry and activate the vector presentation.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer.set_vector_scene_capture_enabled(true);
    apply_mark(&mut renderer);

    // Act: Render at each supported endpoint, then try values outside the camera contract.
    renderer
        .set_vector_zoom_camera(0.25, 0.0, 0.0)
        .expect("the minimum supported zoom should be accepted");
    renderer
        .set_vector_zoom_mode(true)
        .expect("captured geometry should enable vector presentation");
    let minimum_zoom_frame = readback_vector_rgba(&renderer);
    renderer
        .set_vector_zoom_camera(8.0, 0.0, 0.0)
        .expect("the maximum supported zoom should be accepted");
    let maximum_zoom_frame = readback_vector_rgba(&renderer);

    // Assert: Both accepted endpoints produce their respective camera views.
    assert_ne!(minimum_zoom_frame, maximum_zoom_frame);

    // Act: Try out-of-range and non-finite zoom/pan values after the valid maximum.
    let invalid_transforms = [
        (0.0, 0.0, 0.0),
        (0.249, 0.0, 0.0),
        (8.001, 0.0, 0.0),
        (f32::NAN, 0.0, 0.0),
        (f32::INFINITY, 0.0, 0.0),
        (8.0, f32::NAN, 0.0),
        (8.0, 0.0, f32::INFINITY),
    ];
    let mut invalid_inputs_preserve_frame = Vec::new();
    for (zoom, pan_x, pan_y) in invalid_transforms {
        let rejected = matches!(
            renderer.set_vector_zoom_camera(zoom, pan_x, pan_y),
            Err(ExtendedWebGpuRendererError::InvalidVectorZoomCamera)
        );
        invalid_inputs_preserve_frame
            .push(rejected && readback_vector_rgba(&renderer) == maximum_zoom_frame);
    }

    // Assert: Rejected camera inputs leave the last accepted presentation unchanged.
    assert!(
        invalid_inputs_preserve_frame
            .into_iter()
            .all(|preserved| preserved)
    );
}

#[test]
fn moving_vector_camera_does_not_relight_expired_glow() {
    // What: Camera movement preserves both live-glow age and expired-glow state.
    // Why: Vector zoom changes the view, not the timestamp or age of earlier display events.
    // Arrange: Capture a coarse mark in vector mode and measure its full-brightness glow.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer.set_vector_scene_capture_enabled(true);
    apply_mark_with_granularity(&mut renderer, DisplayGranularity::Coarse);
    renderer
        .set_vector_zoom_mode(true)
        .expect("captured geometry should enable vector presentation");
    let peak_frame = readback_vector_rgba(&renderer);
    let peak = pixel_intensity_from_rgba(&peak_frame, 8, 23);
    renderer
        .advance_elapsed(Duration::from_millis(500))
        .expect("the glow should age while the camera remains still");
    renderer
        .compose_frame()
        .expect("the dimmed glow frame should compose");
    let aged_frame = readback_vector_rgba(&renderer);
    let aged = pixel_intensity_from_rgba(&aged_frame, 8, 23);

    // Act: Pan and switch modes during live glow, then move again after its lifetime expires.
    renderer
        .set_vector_zoom_camera(1.0, 4.0, 0.0)
        .expect("the panned camera should be accepted");
    let live_pan_frame = readback_vector_rgba(&renderer);
    let live_pan = pixel_intensity_from_rgba(&live_pan_frame, 12, 23);
    renderer
        .set_vector_zoom_mode(false)
        .expect("Raster mode should remain available during capture");
    renderer
        .set_vector_zoom_mode(true)
        .expect("Vector mode should resume the retained scene");
    let mode_switch_frame = readback_vector_rgba(&renderer);
    let mode_switch = pixel_intensity_from_rgba(&mode_switch_frame, 12, 23);
    renderer
        .advance_elapsed(Duration::from_millis(600))
        .expect("the glow should expire according to its original timestamp");
    renderer
        .compose_frame()
        .expect("the expired-glow frame should compose");
    let expired_frame = readback_vector_rgba(&renderer);
    let expired = pixel_intensity_from_rgba(&expired_frame, 12, 23);
    renderer
        .set_vector_zoom_camera(1.0, 8.0, 0.0)
        .expect("the second camera pan should be accepted");
    let moved_frame = readback_vector_rgba(&renderer);
    let moved = pixel_intensity_from_rgba(&moved_frame, 16, 23);

    // Assert: Camera movement keeps the original live decay and never restamps the expired mark.
    assert_eq!(peak, 255);
    assert!(aged > 240 && aged < peak);
    assert_eq!(live_pan, aged);
    assert_eq!(mode_switch, aged);
    assert!(expired < aged);
    assert_eq!(moved, expired);
}

#[test]
fn vector_scene_storage_is_opt_in_and_released_when_disabled() {
    // What: Retained scene memory appears only while capture is enabled.
    // Why: The normal Extended WebGPU path must not retain vector-only scene data.
    // Arrange: Submit an ordinary stored mark before arming vector-scene capture.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    assert_eq!(renderer.vector_scene_memory_usage(), None);
    apply_mark(&mut renderer);
    assert_eq!(renderer.vector_scene_memory_usage(), None);

    // Act: Enable capture for a stored mark, then release the scene.
    renderer.set_vector_scene_capture_enabled(true);
    let initial_memory = renderer
        .vector_scene_memory_usage()
        .expect("enabling capture should allocate scene state");
    apply_mark(&mut renderer);
    let recorded_memory = renderer
        .vector_scene_memory_usage()
        .expect("scene memory should be measurable while enabled");
    renderer.set_vector_scene_capture_enabled(false);

    // Assert: Recording grows measured storage and disabling releases the scene.
    assert!(recorded_memory.0 > initial_memory.0);
    assert!(recorded_memory.1 >= recorded_memory.0);
    assert_eq!(renderer.vector_scene_memory_usage(), None);
}

#[test]
fn near_white_glow_decays_into_normal_webgpu_frame() {
    // What: A near-white mark decays to the exact RGBA frame produced by the standard renderer.
    // Why: Extended WebGPU adds temporary glow without changing the lasting drawing.
    // Arrange: Compare a coarse mark with the standard WebGPU output for the same operation.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    let normal_frame = normal_webgpu_mark_frame(DisplayGranularity::Coarse);
    let center_offset = (23 * WIDTH as usize + 8) * 4;
    let normal = normal_frame[center_offset + 1];
    apply_mark_with_granularity(&mut renderer, DisplayGranularity::Coarse);
    let initial_frame = readback_rgba(&renderer);
    let initial = initial_frame[center_offset + 1];
    let initial_red = initial_frame[center_offset];
    let initial_blue = initial_frame[center_offset + 2];
    assert_eq!(pixel_intensity(&renderer, 0, 0), 0);

    // Act: Advance halfway through glow decay, then beyond its duration.
    renderer
        .advance_elapsed(Duration::from_millis(500))
        .expect("elapsed time should update the phosphor clock");
    renderer
        .compose_frame()
        .expect("the dimmed frame should compose");
    let halfway_frame = readback_rgba(&renderer);
    let halfway = halfway_frame[center_offset + 1];
    renderer
        .advance_elapsed(Duration::from_millis(501))
        .expect("elapsed time should continue to advance");
    renderer
        .compose_frame()
        .expect("the steady-state frame should compose");
    let steady_state_frame = readback_rgba(&renderer);

    // Assert: The glow starts near white and every final RGBA pixel matches standard WebGPU.
    assert_eq!(initial, 255);
    assert!(initial_red >= 240);
    assert!(initial_blue >= 240);
    assert_eq!(initial_frame[center_offset + 3], 255);
    assert!(halfway > normal && halfway < initial);
    assert!(halfway_frame[center_offset] < initial_red);
    assert!(halfway_frame[center_offset + 2] < initial_blue);
    assert_eq!(steady_state_frame, normal_frame);
    assert_eq!(pixel_intensity(&renderer, 0, 0), 0);
}

#[test]
fn recent_strokes_survive_the_phosphor_clock_wrap() {
    // What: A recent stroke remains visible when the 24-bit millisecond clock wraps.
    // Why: Modular timestamps are unambiguous for strokes younger than the lifetime bound.
    // Arrange: Move to just before wrap while no phosphor pixels are lit.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer
        .advance_elapsed(Duration::from_millis(u64::from(TICK_MASK - 100)))
        .expect("the phosphor clock should advance to the wrap boundary");
    renderer
        .compose_frame()
        .expect("the empty frame should remain black");
    apply_mark_with_granularity(&mut renderer, DisplayGranularity::Coarse);
    let initial = center_intensity(&renderer);
    let revision_before_wrap = renderer.revision();

    // Act: Age the new stroke across the clock wrap by 250 ms.
    renderer
        .advance_elapsed(Duration::from_millis(250))
        .expect("the clock should advance across wrap");
    renderer
        .compose_frame()
        .expect("the recently hit pixel should remain present");

    // Assert: The mark dims but remains visible, and the wrapped frame is presented.
    let across_wrap = center_intensity(&renderer);
    assert!(across_wrap > 0 && across_wrap < initial);
    assert!(renderer.revision() > revision_before_wrap);
}

#[test]
fn selected_speed_scales_elapsed_phosphor_time() {
    // What: The selected CPU speed accelerates phosphor aging for the same host interval.
    // Why: Decay duration is inversely proportional to the current speed multiplier.
    // Arrange: Stamp a mark and set the renderer to the maximum supported speed.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    apply_mark(&mut renderer);
    renderer.set_speed_factor(100.0);

    // Act: Supply 11 ms of host time to the renderer.
    renderer
        .advance_elapsed(Duration::from_millis(11))
        .expect("paused host time should age the phosphor");
    renderer
        .compose_frame()
        .expect("the steady-state frame should compose");

    // Assert: The selected speed removes only the glow, leaving normal WebGPU intensity.
    assert_eq!(
        center_intensity(&renderer),
        normal_webgpu_mark_intensity(DisplayGranularity::Fine)
    );
}

#[test]
fn erase_clears_old_phosphor_and_preserves_a_new_mark() {
    // What: Completed erase removes old phosphor but preserves a mark drawn during its delay.
    // Why: The delayed erase callback must not remove geometry produced after erase starts.
    // Arrange: Draw a mark, let it age halfway, then draw the same mark again.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    apply_mark(&mut renderer);
    renderer
        .advance_elapsed(Duration::from_millis(500))
        .expect("the first mark should age");
    apply_mark(&mut renderer);
    renderer
        .advance_elapsed(Duration::from_millis(500))
        .expect("the redrawn mark should age from its new hit");
    renderer
        .compose_frame()
        .expect("the redrawn frame should compose");
    let redrawn = center_intensity(&renderer);
    assert!(redrawn > 0);

    // Act: Begin the machine's erase interval.
    renderer
        .apply_display_operation(
            DisplayOperation::BeginErase,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("erase should show its active flash");
    renderer
        .compose_frame()
        .expect("the erase flash should compose");

    // Assert: The machine's erase interval is shown as a full green flash.
    assert_eq!(center_intensity(&renderer), 255);

    // Act: Draw a new mark during the erase interval, then complete the erase operation.
    apply_mark(&mut renderer);
    renderer
        .apply_display_operation(
            DisplayOperation::CompleteErase,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("erase should clear the phosphor field");
    renderer
        .compose_frame()
        .expect("the erased frame should compose");

    // Assert: The new mark remains visible after the erase flash is removed.
    assert!(center_intensity(&renderer) > 0);
}

#[test]
fn transient_glow_follows_the_standard_renderer_refresh_lifecycle() {
    // What: A cursor's temporary glow disappears after its normal refresh lifetime.
    // Why: Extended WebGPU must preserve standard transient-versus-stored display behavior.
    // Arrange: Draw a transient cursor mark and confirm its peak is full green.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer
        .apply_display_operation(
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
            },
            DisplayEventBatch {
                events: vec![DisplayEvent::TransientCursorMark(DisplayPoint {
                    x: 8,
                    y: 8,
                })],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("the transient cursor operation should be accepted");
    renderer
        .compose_frame()
        .expect("the cursor glow should compose");
    assert_eq!(center_intensity(&renderer), 255);

    // Act: Apply the standard 100 ms refresh, then let the afterglow finish.
    renderer
        .refresh_display()
        .expect("standard refresh should remove the normal cursor layer");
    assert_eq!(center_intensity(&renderer), 255);
    renderer
        .advance_elapsed(Duration::from_millis(1001))
        .expect("elapsed time should finish the transient glow");
    renderer
        .compose_frame()
        .expect("the empty steady-state frame should compose");

    // Assert: No transient mark remains once both its normal and glow layers expire.
    assert_eq!(center_intensity(&renderer), 0);
}

#[test]
fn transient_vector_glow_expires_without_erasing_geometry() {
    // What: A vector keeps its base stroke after the transient glow expires.
    // Why: Glow is an additive visual effect and must not remove drawn geometry.
    // Arrange: Capture one transiently glowing line in the retained scene.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer.set_vector_scene_capture_enabled(true);
    apply_mark_batch(
        &mut renderer,
        DisplayEventBatch {
            events: vec![DisplayEvent::TransientVector {
                from: DisplayPoint { x: 4, y: 8 },
                to: DisplayPoint { x: 27, y: 8 },
            }],
            dropped: false,
        },
        DisplayGranularity::Fine,
    );
    let peak = pixel_intensity(&renderer, 16, 23);
    assert_eq!(peak, 255);

    // Act: Refresh, let the glow expire, and then render the retained scene in Vector mode.
    renderer
        .refresh_display()
        .expect("the display refresh should preserve the line");
    renderer
        .advance_elapsed(Duration::from_millis(1001))
        .expect("elapsed time should expire the line's glow");
    renderer
        .compose_frame()
        .expect("the persistent raster line should compose");
    let raster_after_glow = pixel_intensity(&renderer, 16, 23);
    renderer
        .set_vector_zoom_mode(true)
        .expect("captured geometry should enable vector presentation");
    let vector_frame = readback_vector_rgba(&renderer);
    let vector_after_glow = pixel_intensity_from_rgba(&vector_frame, 16, 23);

    // Assert: Both presentations retain a steady stroke below the expired glow peak.
    assert!(raster_after_glow > 0);
    assert!(raster_after_glow < peak);
    assert!(vector_after_glow > 0);
    assert!(vector_after_glow < peak);
}

#[test]
fn decay_parameters_accept_all_supported_curve_shapes_and_reject_invalid_values() {
    // What: Renderer construction validates duration and curve parameters.
    // Why: Threshold-based curves need defined bounds so “black” is deterministic.
    // Arrange: Use a native device and prepare valid and invalid parameter sets.
    let (device, queue) = native_gpu_context().expect("native WebGPU should be available");
    let valid = [
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::Linear,
        },
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::Exponential {
                black_threshold: 1.0 / 255.0,
            },
        },
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::Exponential {
                black_threshold: 0.25,
            },
        },
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::InversePower {
                power: 2.0,
                black_threshold: 1.0 / 255.0,
            },
        },
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::InversePower {
                power: 2.0,
                black_threshold: 0.25,
            },
        },
    ];

    // Act: Measure each curve halfway through its lifetime, then try an invalid threshold.
    let mut intensities = Vec::new();
    let mut initial_intensities = Vec::new();
    for parameters in valid {
        let mut renderer = ExtendedWebGpuRenderer::with_decay_parameters(
            Arc::clone(&device),
            Arc::clone(&queue),
            WIDTH,
            HEIGHT,
            parameters,
        )
        .expect("supported decay parameters should create a renderer");
        apply_mark_with_granularity(&mut renderer, DisplayGranularity::Coarse);
        initial_intensities.push(center_intensity(&renderer));
        renderer
            .advance_elapsed(Duration::from_secs(1))
            .expect("the selected decay curve should accept elapsed time");
        renderer
            .compose_frame()
            .expect("the decayed frame should compose");
        intensities.push(center_intensity(&renderer));
    }
    let invalid_parameters = [
        DecayParameters {
            duration: Duration::ZERO,
            curve: DecayCurve::Linear,
        },
        DecayParameters {
            duration: Duration::from_millis(u64::from(TICK_MASK / 2) + 1),
            curve: DecayCurve::Linear,
        },
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::Exponential {
                black_threshold: 1.0,
            },
        },
        DecayParameters {
            duration: Duration::from_secs(2),
            curve: DecayCurve::InversePower {
                power: 0.0,
                black_threshold: 1.0 / 255.0,
            },
        },
    ];
    let invalid = invalid_parameters.map(|parameters| {
        ExtendedWebGpuRenderer::with_decay_parameters(
            Arc::clone(&device),
            Arc::clone(&queue),
            WIDTH,
            HEIGHT,
            parameters,
        )
    });

    // Assert: Linear decay halves glow above baseline, thresholds affect both curves, invalid bounds fail.
    let normal = f32::from(normal_webgpu_mark_intensity(DisplayGranularity::Coarse));
    let peak = f32::from(initial_intensities[0]);
    let linear_glow = (f32::from(intensities[0]) - normal) / (peak - normal);
    assert!((linear_glow - 0.5).abs() <= 0.05);
    assert!(intensities[0] > intensities[1]);
    assert!(intensities[1] < intensities[2]);
    assert!(intensities[2] > intensities[3]);
    assert!(intensities[3] < intensities[4]);
    assert!(intensities[4] > 0);
    assert!(invalid.into_iter().all(|result| matches!(
        result,
        Err(ExtendedWebGpuRendererError::InvalidDecayParameters)
    )));
}

#[test]
fn screen_copy_flushes_unpresented_phosphor_strokes() {
    // What: A screen-copy operation captures strokes staged earlier in the same display batch.
    // Why: Machine-visible copies must preserve operation order even before a browser frame.
    // Arrange: Stage a mark without finishing the display batch.
    let mut renderer = test_renderer().expect("native WebGPU should be available");
    renderer
        .apply_display_operation(
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
            },
            stored_mark(),
            DisplayGranularity::Fine,
        )
        .expect("mark operation should be accepted");

    // Act: Request a screen copy before an explicit presentation flush.
    renderer
        .apply_display_operation(
            DisplayOperation::ScreenCopy,
            DisplayEventBatch {
                events: Vec::new(),
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("screen copy should flush the staged mark");
    let (copies, dropped) = renderer.take_screen_copies();

    // Assert: The event-time copy contains the mark and no copy was dropped.
    assert_eq!(dropped, 0);
    assert_eq!(copies.len(), 1);
    assert!(copies[0][23 * WIDTH as usize + 8] > 0);
}
