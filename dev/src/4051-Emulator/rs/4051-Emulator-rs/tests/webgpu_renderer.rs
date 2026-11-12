#![cfg(feature = "webgpu")]

use std::sync::Arc;

use tek4051_emulator::emulation::webgpu::{
    VectorSegment, WebGpuRendererError, WebGpuVectorRenderer,
};

fn native_gpu_context() -> Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>), String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|error| error.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Tektronix 4051 native renderer test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    Ok((Arc::new(device), Arc::new(queue)))
}

fn pixel_at(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

// What: Rejects an invalid public render batch without changing the existing frame.
// Why: Validation errors must be surfaced before GPU submission or revision changes.
#[test]
#[ignore = "requires a native WebGPU adapter"]
fn invalid_vector_batch_preserves_the_current_frame() {
    // Arrange: Render a valid line and retain its revision and pixel data as the baseline.
    let (device, queue) = native_gpu_context().expect("native WebGPU adapter and device");
    let mut renderer =
        WebGpuVectorRenderer::new(device, queue, 32, 32).expect("offscreen target should fit");
    renderer
        .render_segments(&[VectorSegment::new([4.0, 4.0], [27.0, 25.0], 1.0, 0.75)])
        .expect("valid vector batch should render");
    let revision_before = renderer.revision();
    let frame_before = renderer
        .readback_rgba()
        .expect("baseline frame should read back");

    // Act: Submit one valid and one invalid segment through the public renderer API.
    let result = renderer.render_segments(&[
        VectorSegment::new([2.0, 2.0], [8.0, 8.0], 1.0, 0.5),
        VectorSegment::new([4.0, 4.0], [27.0, 25.0], 0.0, 0.75),
    ]);

    // Assert: The error identifies the segment without changing its prior rendering.
    // Neither the frame contents nor revision advances on a rejected batch.
    assert_eq!(
        result,
        Err(WebGpuRendererError::InvalidSegment { index: 1 })
    );
    assert_eq!(renderer.revision(), revision_before);
    assert_eq!(
        renderer
            .readback_rgba()
            .expect("frame should remain readable"),
        frame_before
    );
}

// What: Checks vector coverage, maximum-intensity overlap, and clear-on-render behavior.
// Why: The offscreen pass must produce antialiased pixels without retaining stale frames or dimming overlaps.
#[test]
#[ignore = "requires a native WebGPU adapter"]
fn native_gpu_renders_antialiased_vector_segments() {
    // Arrange: Let the native host create GPU resources and pass them to emulation-owned rendering.
    let (device, queue) = native_gpu_context().expect("native WebGPU adapter and device");
    let mut renderer =
        WebGpuVectorRenderer::new(device, queue, 32, 32).expect("offscreen target should fit");
    let segments = [VectorSegment::new([4.0, 4.0], [27.0, 25.0], 1.0, 1.0)];

    // Act: Rasterize the vector and read back the native proof frame.
    renderer
        .render_segments(&segments)
        .expect("vector batch should be valid");
    let pixels = renderer
        .readback_rgba()
        .expect("frame readback should succeed");

    // Assert: Expected line pixels are covered, an edge is antialiased, and distant pixels stay clear.
    assert_eq!(pixels.len(), 32 * 32 * 4);
    let center = pixel_at(&pixels, 32, 16, 15);
    assert!(center[1] >= 240 && center[3] >= 240);
    let edge = pixel_at(&pixels, 32, 16, 14);
    assert!((1..u8::MAX).contains(&edge[1]));
    assert_eq!(edge[1], edge[3]);
    assert_eq!(pixel_at(&pixels, 32, 2, 2), [0, 0, 0, 0]);

    // Act: Render overlapping weak/strong vectors in both orders, then render the strong vector alone.
    let weaker = VectorSegment::new([4.0, 4.0], [27.0, 25.0], 1.0, 0.35);
    let stronger = VectorSegment::new([4.0, 4.0], [27.0, 25.0], 1.0, 0.75);
    renderer
        .render_segments(&[weaker, stronger])
        .expect("overlapping vector batch should be valid");
    let weaker_then_stronger = renderer
        .readback_rgba()
        .expect("weak-then-strong overlap readback should succeed");
    renderer
        .render_segments(&[stronger, weaker])
        .expect("reversed overlapping vector batch should be valid");
    let stronger_then_weaker = renderer
        .readback_rgba()
        .expect("strong-then-weak overlap readback should succeed");
    renderer
        .render_segments(&[stronger])
        .expect("strong vector batch should be valid");
    let stronger_pixels = renderer
        .readback_rgba()
        .expect("strong vector frame readback should succeed");

    // Assert: Maximum blending is order-independent and matches the stronger vector.
    assert_eq!(weaker_then_stronger, stronger_pixels);
    assert_eq!(stronger_then_weaker, stronger_pixels);

    // Act: Submit an empty batch after a populated frame.
    renderer
        .render_segments(&[])
        .expect("empty vector batch should clear the frame");
    let cleared_pixels = renderer
        .readback_rgba()
        .expect("cleared frame readback should succeed");

    // Assert: The new frame is transparent rather than retaining the previous vector.
    assert!(cleared_pixels.iter().all(|channel| *channel == 0));
}
