use std::{sync::Arc, time::Duration};

use super::HostRuntime;
use crate::{
    emulation::{
        display::{DisplayEvent, DisplayEventBatch, DisplayGranularity, DisplayPoint},
        extended_webgpu::ExtendedWebGpuRenderer,
    },
    machine::{
        Machine,
        display::{DisplayControl, DisplayOperation, ScreenCommand},
    },
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
        label: Some("Extended WebGPU paused-runtime test device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .map_err(|error| error.to_string())?;
    Ok((Arc::new(device), Arc::new(queue)))
}

fn renderer_with_mark() -> ExtendedWebGpuRenderer {
    let (device, queue) = native_gpu_context().expect("native WebGPU should be available");
    let mut renderer =
        ExtendedWebGpuRenderer::new(device, queue, 32, 32).expect("renderer should initialize");
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
                events: vec![DisplayEvent::StoredMark {
                    position: DisplayPoint { x: 8, y: 8 },
                    size: 1,
                }],
                dropped: false,
            },
            DisplayGranularity::Fine,
        )
        .expect("mark operation should be accepted");
    renderer
        .finish_display_batch()
        .expect("mark operation should be stamped");
    renderer
        .compose_frame()
        .expect("initial phosphor frame should compose");
    renderer
}

#[test]
fn paused_runtime_advances_extended_phosphor_at_the_selected_speed() {
    // What: Paused runtime time updates the Extended WebGPU phosphor field.
    // Why: Pause suppresses CPU callbacks, but afterglow must continue to decay.
    // Arrange: Select x100 before attaching the marked renderer, then pause the machine.
    let renderer = renderer_with_mark();
    let mut machine = Machine::default();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .set_speed_factor(100.0)
        .expect("x100 should be a valid speed");
    runtime
        .enable_extended_webgpu_renderer(&mut machine, renderer)
        .expect("renderer should attach to an empty display");
    runtime.pause();
    let initial_revision = runtime.presentation_revision;
    let initial_pc = machine.registers().pc;

    // Act: Advance 11 ms while paused, which represents more than one lifetime at x100.
    runtime
        .advance(&mut machine, Duration::from_millis(11))
        .expect("paused advancement should not execute the stopped CPU");

    // Assert: x100 removes the glow while the paused CPU stays fixed and normal intensity remains.
    assert!(runtime.presentation_revision > initial_revision);
    assert_eq!(machine.registers().pc, initial_pc);
    assert!(runtime.render_error().is_none());
    assert!(runtime.extended_webgpu_active());
    let pixels = runtime
        .extended_webgpu_renderer
        .as_ref()
        .expect("Extended WebGPU should remain active")
        .readback_green()
        .expect("the paused phosphor frame should be readable");
    assert_eq!(pixels[23 * 32 + 8], 254);
    assert_eq!(pixels[0], 0);
}

#[test]
fn paused_runtime_speed_changes_update_an_attached_extended_renderer() {
    // What: Changing runtime speed after installation changes phosphor aging.
    // Why: The renderer must follow speed controls throughout the session, not only at setup.
    // Arrange: Attach a marked renderer at x1, then select x100 while paused.
    let mut machine = Machine::default();
    let mut runtime = HostRuntime::start(&mut machine);
    runtime
        .enable_extended_webgpu_renderer(&mut machine, renderer_with_mark())
        .expect("renderer should attach to an empty display");
    runtime.pause();
    runtime
        .set_speed_factor(100.0)
        .expect("x100 should be a valid speed");
    let initial_pc = machine.registers().pc;

    // Act: Let 11 ms of paused host time advance at the newly selected factor.
    runtime
        .advance(&mut machine, Duration::from_millis(11))
        .expect("paused advancement should not execute the stopped CPU");

    // Assert: The glow decays to the normal mark, CPU state is fixed, and rendering remains active.
    assert_eq!(machine.registers().pc, initial_pc);
    assert!(runtime.extended_webgpu_active());
    assert!(runtime.render_error().is_none());
    let pixels = runtime
        .extended_webgpu_renderer
        .as_ref()
        .expect("Extended WebGPU should remain active")
        .readback_green()
        .expect("the paused phosphor frame should be readable");
    assert_eq!(pixels[23 * 32 + 8], 254);
}
