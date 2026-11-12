//! Opt-in WebGPU renderer that adds a decaying highlight over normal WebGPU output.
//!
//! The renderer keeps an unchanged standard WebGPU frame as its steady-state
//! baseline, reuses the vector rasterizer for full-intensity antialiased masks,
//! and stores each pixel's last-hit time and coverage. Composition adds a
//! temporary near-white highlight and decays it to that baseline. The CPU
//! reference renderer continues to own machine operations and semantic events.

use std::{error::Error, fmt, sync::Arc, time::Duration};

use wgpu::util::DeviceExt;

use super::{
    display::{DisplayEvent, DisplayEventBatch, DisplayGranularity, DisplayPoint},
    vector_scene::{VectorScene, VectorSceneCaptureError},
    webgpu::{VectorSegment, WebGpuVectorRenderer},
};
use crate::machine::display::DisplayOperation;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "extended_webgpu/tests.rs"]
mod tests;

const TICK_MASK: u32 = 0x00ff_ffff;
const DEFAULT_DECAY_DURATION: Duration = Duration::from_secs(1);
const DEFAULT_DECAY_CURVE: DecayCurve = DecayCurve::Linear;
#[cfg(not(target_arch = "wasm32"))]
const MAX_PENDING_SCREEN_COPIES: usize = 8;

const STAMP_SHADER: &str = r"
struct RendererUniforms {
    dimensions: vec2<f32>,
    current_tick: u32,
    lifetime_ms: u32,
    curve: u32,
    curve_parameter: f32,
    black_threshold: f32,
    padding: u32,
}

@group(0) @binding(0)
var stroke_mask: texture_2d<f32>;

@group(0) @binding(1)
var<uniform> renderer: RendererUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let coordinate = vec2<i32>(input.position.xy);
    let coverage = textureLoad(stroke_mask, coordinate, 0).g;
    if coverage <= 0.0 {
        discard;
    }

    let tick = renderer.current_tick & 0x00ffffffu;
    return vec4<f32>(
        f32(tick & 255u) / 255.0,
        f32((tick >> 8u) & 255u) / 255.0,
        f32((tick >> 16u) & 255u) / 255.0,
        coverage,
    );
}
";

const COMPOSE_SHADER: &str = r"
struct RendererUniforms {
    dimensions: vec2<f32>,
    current_tick: u32,
    lifetime_ms: u32,
    curve: u32,
    curve_parameter: f32,
    black_threshold: f32,
    padding: u32,
}

@group(0) @binding(0)
var phosphor: texture_2d<f32>;

@group(0) @binding(1)
var normal_frame: texture_2d<f32>;

@group(0) @binding(2)
var<uniform> renderer: RendererUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return output;
}

fn decay_fraction(age_ms: f32, lifetime_ms: f32) -> f32 {
    if age_ms >= lifetime_ms {
        return 0.0;
    }

    let age = age_ms / lifetime_ms;
    var brightness = 0.0;
    if renderer.curve == 0u {
        brightness = 1.0 - age;
    } else if renderer.curve == 1u {
        brightness = pow(renderer.black_threshold, age);
    } else {
        let growth = pow(1.0 / renderer.black_threshold, 1.0 / renderer.curve_parameter) - 1.0;
        brightness = pow(1.0 + age * growth, -renderer.curve_parameter);
    }
    if brightness <= renderer.black_threshold {
        return 0.0;
    }
    return clamp(brightness, 0.0, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let coordinate = vec2<i32>(input.position.xy);
    let normal = textureLoad(normal_frame, coordinate, 0).g;
    let stamp = textureLoad(phosphor, coordinate, 0);
    if stamp.a <= 0.0 {
        return vec4<f32>(0.0, normal, 0.0, normal);
    }

    let bytes = vec3<u32>(round(stamp.rgb * 255.0));
    let last_hit = bytes.x | (bytes.y << 8u) | (bytes.z << 16u);
    let age = (renderer.current_tick - last_hit) & 0x00ffffffu;
    let glow = decay_fraction(f32(age), f32(renderer.lifetime_ms));
    let peak = max(normal, stamp.a);
    let brightness = normal + (peak - normal) * glow;
    let white_tint = 0.95 * stamp.a * glow;
    return vec4<f32>(
        white_tint,
        brightness,
        white_tint,
        max(brightness, white_tint),
    );
}
";

const VECTOR_COMPOSE_SHADER: &str = r"
struct RendererUniforms {
    dimensions: vec2<f32>,
    current_tick: u32,
    lifetime_ms: u32,
    curve: u32,
    curve_parameter: f32,
    black_threshold: f32,
    padding: u32,
    camera_zoom: f32,
    camera_pan: vec2<f32>,
}

@group(0) @binding(0)
var phosphor: texture_2d<f32>;

@group(0) @binding(1)
var vector_scene: texture_2d<f32>;

@group(0) @binding(2)
var<uniform> renderer: RendererUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return output;
}

fn decay_fraction(age_ms: f32, lifetime_ms: f32) -> f32 {
    if age_ms >= lifetime_ms {
        return 0.0;
    }

    let age = age_ms / lifetime_ms;
    var brightness = 0.0;
    if renderer.curve == 0u {
        brightness = 1.0 - age;
    } else if renderer.curve == 1u {
        brightness = pow(renderer.black_threshold, age);
    } else {
        let growth = pow(1.0 / renderer.black_threshold, 1.0 / renderer.curve_parameter) - 1.0;
        brightness = pow(1.0 + age * growth, -renderer.curve_parameter);
    }
    if brightness <= renderer.black_threshold {
        return 0.0;
    }
    return clamp(brightness, 0.0, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let coordinate = vec2<i32>(input.position.xy);
    let normal = textureLoad(vector_scene, coordinate, 0).g;
    let center = renderer.dimensions * 0.5;
    let source_position =
        (vec2<f32>(coordinate) + vec2<f32>(0.5) - center - renderer.camera_pan)
        / renderer.camera_zoom + center;
    var stamp = vec4<f32>(0.0);
    if all(source_position >= vec2<f32>(0.0))
        && all(source_position < renderer.dimensions)
    {
        stamp = textureLoad(phosphor, vec2<i32>(floor(source_position)), 0);
    }
    if stamp.a <= 0.0 {
        return vec4<f32>(0.0, normal, 0.0, normal);
    }

    let bytes = vec3<u32>(round(stamp.rgb * 255.0));
    let last_hit = bytes.x | (bytes.y << 8u) | (bytes.z << 16u);
    let age = (renderer.current_tick - last_hit) & 0x00ffffffu;
    let glow = decay_fraction(f32(age), f32(renderer.lifetime_ms));
    let peak = max(normal, stamp.a);
    let brightness = normal + (peak - normal) * glow;
    let white_tint = 0.95 * stamp.a * glow;
    return vec4<f32>(
        white_tint,
        brightness,
        white_tint,
        max(brightness, white_tint),
    );
}
";

/// Curves used to decay the Extended WebGPU highlight to its normal baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecayCurve {
    /// Reduces the added highlight linearly to zero over the configured duration.
    Linear,
    /// Uses an exponential curve and clamps the highlight at the specified threshold.
    Exponential { black_threshold: f32 },
    /// Uses an inverse-power curve that reaches the specified threshold at the duration.
    InversePower { power: f32, black_threshold: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecayParameters {
    /// Highlight decay duration at speed x1, from 1 ms to half the 24-bit tick range.
    pub duration: Duration,
    /// Function used to convert pixel age to display intensity.
    pub curve: DecayCurve,
}

impl Default for DecayParameters {
    fn default() -> Self {
        Self {
            duration: DEFAULT_DECAY_DURATION,
            curve: DEFAULT_DECAY_CURVE,
        }
    }
}

/// Errors while setting up or using the Extended WebGPU renderer.
#[derive(Debug)]
pub enum ExtendedWebGpuRendererError {
    /// The requested render target is empty or exceeds device limits.
    InvalidDimensions,
    /// The decay duration or curve parameters cannot be represented safely.
    InvalidDecayParameters,
    /// Retained vector-scene storage could not grow for a display event.
    VectorSceneAllocationFailed,
    /// The emulation event queue overflowed before the renderer received it.
    DisplayEventsDropped,
    /// Vector presentation was requested before scene capture was enabled.
    VectorSceneCaptureDisabled,
    /// Camera zoom or pan is not finite or is outside the supported zoom range.
    InvalidVectorZoomCamera,
    /// WebGPU rejected a renderer operation.
    WebGpu(String),
    #[cfg(not(target_arch = "wasm32"))]
    /// The native GPU frame could not be copied for a screen capture.
    Readback(String),
}

impl fmt::Display for ExtendedWebGpuRendererError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions => write!(formatter, "invalid Extended WebGPU dimensions"),
            Self::InvalidDecayParameters => {
                write!(formatter, "invalid Extended WebGPU decay parameters")
            }
            Self::VectorSceneAllocationFailed => {
                write!(formatter, "vector scene storage allocation failed")
            }
            Self::DisplayEventsDropped => {
                write!(
                    formatter,
                    "display events were dropped before phosphor rendering"
                )
            }
            Self::VectorSceneCaptureDisabled => {
                write!(formatter, "vector scene capture must be enabled first")
            }
            Self::InvalidVectorZoomCamera => {
                write!(formatter, "invalid vector-zoom camera transform")
            }
            Self::WebGpu(error) => write!(formatter, "Extended WebGPU operation failed: {error}"),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Readback(error) => write!(formatter, "Extended WebGPU readback failed: {error}"),
        }
    }
}

impl Error for ExtendedWebGpuRendererError {}

/// GPU phosphor overlay that returns each pixel to the standard WebGPU frame.
pub struct ExtendedWebGpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    geometry: WebGpuVectorRenderer,
    baseline_renderer: WebGpuVectorRenderer,
    _stamp_texture: wgpu::Texture,
    stamp_view: wgpu::TextureView,
    _output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    stamp_pipeline: wgpu::RenderPipeline,
    stamp_bind_group: wgpu::BindGroup,
    compose_pipeline: wgpu::RenderPipeline,
    compose_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    current_tick: u32,
    fractional_tick_ms: f64,
    lifetime_ms: u32,
    curve: DecayCurve,
    speed_factor: f64,
    latest_hit_tick: Option<u32>,
    erase_flash: bool,
    pending_segments: Vec<VectorSegment>,
    vector_scene: Option<Box<VectorScene>>,
    vector_zoom: Option<VectorZoomResources>,
    vector_mode: bool,
    camera_zoom: f32,
    camera_pan: [f32; 2],
    vector_scene_dirty: bool,
    vector_frame_dirty: bool,
    frame_dirty: bool,
    revision: u64,
    screen_copies: Vec<Vec<u8>>,
    dropped_screen_copies: usize,
}

struct VectorZoomResources {
    scene_renderer: WebGpuVectorRenderer,
    _output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    compose_pipeline: wgpu::RenderPipeline,
    compose_bind_group: wgpu::BindGroup,
    segments: Vec<VectorSegment>,
}

impl VectorZoomResources {
    fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        stamp_view: &wgpu::TextureView,
        uniform_buffer: &wgpu::Buffer,
    ) -> Result<Self, ExtendedWebGpuRendererError> {
        let scene_renderer = WebGpuVectorRenderer::new(device.clone(), queue, width, height)
            .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        let output_texture = create_texture(
            &device,
            "Tektronix 4051 vector-zoom presentation frame",
            width,
            height,
        );
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let compose_pipeline = create_pipeline(
            &device,
            VECTOR_COMPOSE_SHADER,
            "Tektronix 4051 vector-zoom composition",
            2,
        );
        let compose_bind_group = create_bind_group(
            &device,
            &compose_pipeline,
            &[stamp_view, scene_renderer.target_view()],
            uniform_buffer,
        );
        Ok(Self {
            scene_renderer,
            _output_texture: output_texture,
            output_view,
            compose_pipeline,
            compose_bind_group,
            segments: Vec::new(),
        })
    }
}

impl ExtendedWebGpuRenderer {
    /// Creates an isolated Extended WebGPU target with a one-second linear glow decay at x1.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
    ) -> Result<Self, ExtendedWebGpuRendererError> {
        Self::with_decay(device, queue, width, height, DecayParameters::default())
    }

    /// Creates a renderer with caller-selected glow duration and decay curve.
    pub fn with_decay_parameters(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        parameters: DecayParameters,
    ) -> Result<Self, ExtendedWebGpuRendererError> {
        Self::with_decay(device, queue, width, height, parameters)
    }

    fn with_decay(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        parameters: DecayParameters,
    ) -> Result<Self, ExtendedWebGpuRendererError> {
        if width == 0
            || height == 0
            || width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(ExtendedWebGpuRendererError::InvalidDimensions);
        }
        let lifetime_ms = u32::try_from(parameters.duration.as_millis())
            .ok()
            .filter(|duration| (1..=TICK_MASK / 2).contains(duration))
            .ok_or(ExtendedWebGpuRendererError::InvalidDecayParameters)?;
        validate_curve(parameters.curve)?;

        let baseline_renderer =
            WebGpuVectorRenderer::new(Arc::clone(&device), Arc::clone(&queue), width, height)
                .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        let geometry =
            WebGpuVectorRenderer::new(Arc::clone(&device), Arc::clone(&queue), width, height)
                .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        let stamp_texture =
            create_texture(&device, "Tektronix 4051 phosphor hit times", width, height);
        let stamp_view = stamp_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let output_texture = create_texture(
            &device,
            "Tektronix 4051 extended phosphor frame",
            width,
            height,
        );
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut uniform_bytes = vec![0; 48];
        uniform_bytes[0..4].copy_from_slice(&(width as f32).to_ne_bytes());
        uniform_bytes[4..8].copy_from_slice(&(height as f32).to_ne_bytes());
        uniform_bytes[12..16].copy_from_slice(&lifetime_ms.to_ne_bytes());
        uniform_bytes[32..36].copy_from_slice(&1.0_f32.to_ne_bytes());
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tektronix 4051 extended phosphor parameters"),
            contents: &uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let stamp_pipeline =
            create_pipeline(&device, STAMP_SHADER, "Tektronix 4051 phosphor stamp", 1);
        let stamp_bind_group = create_bind_group(
            &device,
            &stamp_pipeline,
            &[geometry.target_view()],
            &uniform_buffer,
        );
        let compose_pipeline = create_pipeline(
            &device,
            COMPOSE_SHADER,
            "Tektronix 4051 phosphor composition",
            2,
        );
        let compose_bind_group = create_bind_group(
            &device,
            &compose_pipeline,
            &[&stamp_view, baseline_renderer.target_view()],
            &uniform_buffer,
        );
        let mut renderer = Self {
            device,
            queue,
            geometry,
            baseline_renderer,
            _stamp_texture: stamp_texture,
            stamp_view,
            _output_texture: output_texture,
            output_view,
            uniform_buffer,
            stamp_pipeline,
            stamp_bind_group,
            compose_pipeline,
            compose_bind_group,
            width,
            height,
            current_tick: 0,
            fractional_tick_ms: 0.0,
            lifetime_ms,
            curve: parameters.curve,
            speed_factor: 1.0,
            latest_hit_tick: None,
            erase_flash: false,
            pending_segments: Vec::new(),
            vector_scene: None,
            vector_zoom: None,
            vector_mode: false,
            camera_zoom: 1.0,
            camera_pan: [0.0, 0.0],
            vector_scene_dirty: true,
            vector_frame_dirty: true,
            frame_dirty: true,
            revision: 0,
            screen_copies: Vec::new(),
            dropped_screen_copies: 0,
        };
        renderer.update_uniforms();
        renderer.clear_stamps();
        renderer.compose_frame()?;
        Ok(renderer)
    }

    /// Returns the offscreen frame view used by the browser or native presentation.
    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    /// Returns the active browser presentation target; screen copies use `target_view`.
    pub fn presentation_view(&self) -> &wgpu::TextureView {
        if self.vector_mode {
            &self
                .vector_zoom
                .as_ref()
                .expect("vector mode has an allocated presentation target")
                .output_view
        } else {
            &self.output_view
        }
    }

    pub(crate) const fn vector_zoom_active(&self) -> bool {
        self.vector_mode
    }

    /// Returns the renderer's presentation revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Enables or releases renderer-independent display-scene recording.
    ///
    /// A disabled-to-enabled transition allocates a fresh scene; the caller owns
    /// the hard-reset boundary and enables capture before reset operations arrive.
    pub fn set_vector_scene_capture_enabled(&mut self, enabled: bool) {
        if enabled {
            self.vector_scene
                .get_or_insert_with(|| Box::new(VectorScene::default()));
        } else {
            self.vector_scene = None;
            self.vector_zoom = None;
            self.vector_mode = false;
            self.vector_scene_dirty = true;
            self.vector_frame_dirty = true;
        }
    }

    /// Selects vector presentation while retaining the camera-independent screen-copy frame.
    pub(crate) fn set_vector_zoom_mode(
        &mut self,
        enabled: bool,
    ) -> Result<(), ExtendedWebGpuRendererError> {
        if enabled && self.vector_scene.is_none() {
            return Err(ExtendedWebGpuRendererError::VectorSceneCaptureDisabled);
        }
        if enabled && self.vector_zoom.is_none() {
            self.vector_zoom = Some(VectorZoomResources::new(
                Arc::clone(&self.device),
                Arc::clone(&self.queue),
                self.width,
                self.height,
                &self.stamp_view,
                &self.uniform_buffer,
            )?);
            self.vector_scene_dirty = true;
        }
        if self.vector_mode == enabled {
            return Ok(());
        }
        self.vector_mode = enabled;
        self.vector_frame_dirty = true;
        if enabled {
            self.vector_scene_dirty = true;
            self.compose_frame()?;
        }
        Ok(())
    }

    /// Updates the virtual camera without modifying retained display events or glow stamps.
    pub(crate) fn set_vector_zoom_camera(
        &mut self,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
    ) -> Result<(), ExtendedWebGpuRendererError> {
        if !zoom.is_finite()
            || !(0.25..=8.0).contains(&zoom)
            || !pan_x.is_finite()
            || !pan_y.is_finite()
        {
            return Err(ExtendedWebGpuRendererError::InvalidVectorZoomCamera);
        }
        if self.camera_zoom == zoom && self.camera_pan == [pan_x, pan_y] {
            return Ok(());
        }
        self.camera_zoom = zoom;
        self.camera_pan = [pan_x, pan_y];
        self.update_uniforms();
        if self.vector_mode {
            self.vector_scene_dirty = true;
            self.vector_frame_dirty = true;
            self.compose_frame()?;
        }
        Ok(())
    }

    /// Returns current and peak retained event-buffer bytes while capture is enabled.
    pub fn vector_scene_memory_usage(&self) -> Option<(usize, usize)> {
        self.vector_scene
            .as_ref()
            .map(|scene| scene.memory_usage())
            .map(|usage| (usage.current_bytes, usage.peak_bytes))
    }

    /// Applies a validated speed multiplier to future elapsed phosphor time.
    pub(crate) fn set_speed_factor(&mut self, speed_factor: f64) {
        self.speed_factor = speed_factor;
    }

    /// Applies an ordered machine operation and its corresponding semantic display events.
    pub(crate) fn apply_display_operation(
        &mut self,
        operation: DisplayOperation,
        events: DisplayEventBatch,
        granularity: DisplayGranularity,
    ) -> Result<(), ExtendedWebGpuRendererError> {
        if events.dropped {
            return Err(ExtendedWebGpuRendererError::DisplayEventsDropped);
        }

        let vector_scene_changed = matches!(
            operation,
            DisplayOperation::BeginErase | DisplayOperation::CompleteErase
        ) || events.events.iter().any(|event| {
            matches!(
                *event,
                DisplayEvent::StoredMark { .. }
                    | DisplayEvent::StoredVector { .. }
                    | DisplayEvent::TransientVector { .. }
            )
        });
        if let Some(scene) = self.vector_scene.as_mut() {
            scene
                .apply_display_operation(operation, &events, granularity)
                .map_err(|error| match error {
                    VectorSceneCaptureError::AllocationFailed => {
                        ExtendedWebGpuRendererError::VectorSceneAllocationFailed
                    }
                    VectorSceneCaptureError::DisplayEventsDropped => {
                        ExtendedWebGpuRendererError::DisplayEventsDropped
                    }
                })?;
        }

        if matches!(operation, DisplayOperation::ScreenCopy) {
            self.baseline_renderer
                .finish_display_batch()
                .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        } else {
            self.baseline_renderer
                .apply_display_operation(
                    operation,
                    DisplayEventBatch {
                        events: events.events.clone(),
                        dropped: false,
                    },
                    granularity,
                )
                .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        }

        for event in events.events {
            match event {
                DisplayEvent::TransientCursorMark(position) => self
                    .pending_segments
                    .extend(cursor_mark_segments(position, granularity, self.height)),
                DisplayEvent::StoredMark { position, size } => self.pending_segments.push(
                    mark_segment(position, size, stored_intensity(granularity), self.height),
                ),
                DisplayEvent::TransientVector { from, to } => self.pending_segments.push(
                    vector_segment(from, to, 1, cursor_intensity(granularity), self.height),
                ),
                DisplayEvent::StoredVector { from, to, size } => self.pending_segments.push(
                    vector_segment(from, to, size, stored_intensity(granularity), self.height),
                ),
                DisplayEvent::Erase => {
                    self.erase_flash = true;
                    self.frame_dirty = true;
                }
            }
        }
        if vector_scene_changed {
            self.vector_scene_dirty = true;
            self.vector_frame_dirty = true;
        }

        match operation {
            DisplayOperation::BeginErase => {
                // Discard pre-erase glow; later display events repopulate the retained scene.
                self.pending_segments.clear();
                self.latest_hit_tick = None;
                self.clear_stamps();
                self.erase_flash = true;
                self.frame_dirty = true;
            }
            DisplayOperation::CompleteErase => {
                self.erase_flash = false;
                self.frame_dirty = true;
            }
            DisplayOperation::ScreenCopy => {
                self.finish_display_batch()?;
                self.compose_frame()?;
                self.capture_screen_copy()?;
                return Ok(());
            }
            DisplayOperation::ScreenCommand { .. } => {}
        }
        Ok(())
    }

    /// Flushes the normal frame and rasterizes staged full-intensity glow masks.
    pub(crate) fn finish_display_batch(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        self.baseline_renderer
            .finish_display_batch()
            .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        if self.pending_segments.is_empty() {
            return Ok(());
        }
        let glow_segments = self
            .pending_segments
            .iter()
            .map(|segment| VectorSegment {
                intensity: 1.0,
                ..*segment
            })
            .collect::<Vec<_>>();
        self.geometry
            .render_segments(&glow_segments)
            .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        self.stamp_current_mask()?;
        self.pending_segments.clear();
        self.latest_hit_tick = Some(self.current_tick);
        self.frame_dirty = true;
        Ok(())
    }

    /// Applies standard renderer transient-refresh semantics while preserving glow timestamps.
    pub(crate) fn refresh_display(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        self.finish_display_batch()?;
        self.baseline_renderer
            .refresh_display()
            .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))?;
        if let Some(scene) = self.vector_scene.as_mut() {
            scene.refresh_display();
        }
        self.frame_dirty = true;
        self.compose_frame()
    }

    /// Ages the field by host elapsed time, including elapsed time while paused.
    pub(crate) fn advance_elapsed(
        &mut self,
        elapsed: Duration,
    ) -> Result<(), ExtendedWebGpuRendererError> {
        if elapsed.is_zero() {
            return Ok(());
        }
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0 * self.speed_factor;
        let total_ticks = self.fractional_tick_ms + elapsed_ms;
        let whole_ticks = total_ticks.floor();
        self.fractional_tick_ms = total_ticks - whole_ticks;
        let tick_count = whole_ticks.min(u64::MAX as f64) as u64;
        if tick_count == 0 {
            return Ok(());
        }

        if tick_count >= u64::from(self.lifetime_ms) {
            self.clear_stamps();
            self.latest_hit_tick = None;
        }
        self.current_tick =
            ((u64::from(self.current_tick) + tick_count) & u64::from(TICK_MASK)) as u32;
        self.update_uniforms();

        if self.erase_flash {
            return Ok(());
        }
        let Some(last_hit) = self.latest_hit_tick else {
            return Ok(());
        };
        if self.current_tick.wrapping_sub(last_hit) & TICK_MASK >= self.lifetime_ms {
            self.clear_stamps();
            self.latest_hit_tick = None;
        }
        self.frame_dirty = true;
        Ok(())
    }

    /// Composes the current phosphor field if time or display state changed.
    pub(crate) fn compose_frame(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        self.finish_display_batch()?;
        let raster_changed = self.frame_dirty;
        if raster_changed {
            self.compose_raster_frame();
            self.frame_dirty = false;
        }
        let vector_changed = self.vector_mode && (raster_changed || self.vector_frame_dirty);
        if vector_changed {
            self.compose_vector_frame()?;
            self.vector_frame_dirty = false;
        }
        if raster_changed || vector_changed {
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }

    fn compose_raster_frame(&self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 extended phosphor frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tektronix 4051 extended phosphor composition"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.erase_flash {
                            wgpu::Color {
                                r: 0.0,
                                g: 1.0,
                                b: 0.0,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color::BLACK
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !self.erase_flash {
                pass.set_pipeline(&self.compose_pipeline);
                pass.set_bind_group(0, &self.compose_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    fn compose_vector_frame(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        if self.vector_scene_dirty {
            self.rasterize_vector_scene()?;
            self.vector_scene_dirty = false;
        }
        let erase_flash = self.erase_flash;
        let resources = self
            .vector_zoom
            .as_mut()
            .expect("vector mode has an allocated presentation target");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 vector-zoom phosphor frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tektronix 4051 vector-zoom composition"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &resources.output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if erase_flash {
                            wgpu::Color {
                                r: 0.0,
                                g: 1.0,
                                b: 0.0,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color::BLACK
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !erase_flash {
                pass.set_pipeline(&resources.compose_pipeline);
                pass.set_bind_group(0, &resources.compose_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn rasterize_vector_scene(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        let scene = self
            .vector_scene
            .as_ref()
            .ok_or(ExtendedWebGpuRendererError::VectorSceneCaptureDisabled)?;
        let resources = self
            .vector_zoom
            .as_mut()
            .expect("vector mode has an allocated scene renderer");
        resources.segments.clear();
        let zoom = self.camera_zoom;
        let pan = self.camera_pan;
        for (event, granularity) in scene.persistent_geometry_events() {
            let mut segment = match event {
                DisplayEvent::StoredMark { position, size } => {
                    mark_segment(position, size, stored_intensity(granularity), self.height)
                }
                DisplayEvent::StoredVector { from, to, size } => {
                    vector_segment(from, to, size, stored_intensity(granularity), self.height)
                }
                DisplayEvent::TransientVector { from, to } => {
                    vector_segment(from, to, 1, stored_intensity(granularity), self.height)
                }
                _ => continue,
            };
            transform_segment(&mut segment, zoom, pan, self.width, self.height);
            resources
                .segments
                .try_reserve(1)
                .map_err(|_| ExtendedWebGpuRendererError::VectorSceneAllocationFailed)?;
            resources.segments.push(segment);
        }
        resources
            .scene_renderer
            .render_segments(&resources.segments)
            .map_err(|error| ExtendedWebGpuRendererError::WebGpu(error.to_string()))
    }

    pub(crate) fn take_screen_copies(&mut self) -> (Vec<Vec<u8>>, usize) {
        (
            std::mem::take(&mut self.screen_copies),
            std::mem::take(&mut self.dropped_screen_copies),
        )
    }

    fn stamp_current_mask(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        self.update_uniforms();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 phosphor stroke timestamp"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tektronix 4051 phosphor stroke timestamp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.stamp_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.stamp_pipeline);
            pass.set_bind_group(0, &self.stamp_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn clear_stamps(&mut self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 clear phosphor timestamps"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tektronix 4051 clear phosphor timestamps"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.stamp_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
        self.frame_dirty = true;
    }

    fn update_uniforms(&self) {
        let (curve, parameter, threshold) = match self.curve {
            DecayCurve::Linear => (0u32, 0.0, 0.0),
            DecayCurve::Exponential { black_threshold } => (1, 0.0, black_threshold),
            DecayCurve::InversePower {
                power,
                black_threshold,
            } => (2, power, black_threshold),
        };
        let mut bytes = vec![0; 48];
        bytes[0..4].copy_from_slice(&(self.width as f32).to_ne_bytes());
        bytes[4..8].copy_from_slice(&(self.height as f32).to_ne_bytes());
        bytes[8..12].copy_from_slice(&self.current_tick.to_ne_bytes());
        bytes[12..16].copy_from_slice(&self.lifetime_ms.to_ne_bytes());
        bytes[16..20].copy_from_slice(&curve.to_ne_bytes());
        bytes[20..24].copy_from_slice(&parameter.to_ne_bytes());
        bytes[24..28].copy_from_slice(&threshold.to_ne_bytes());
        bytes[32..36].copy_from_slice(&self.camera_zoom.to_ne_bytes());
        bytes[40..44].copy_from_slice(&self.camera_pan[0].to_ne_bytes());
        bytes[44..48].copy_from_slice(&self.camera_pan[1].to_ne_bytes());
        self.queue.write_buffer(&self.uniform_buffer, 0, &bytes);
    }

    fn capture_screen_copy(&mut self) -> Result<(), ExtendedWebGpuRendererError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pixels = self.readback_green()?;
            if self.screen_copies.len() == MAX_PENDING_SCREEN_COPIES {
                self.dropped_screen_copies = self.dropped_screen_copies.saturating_add(1);
            } else {
                self.screen_copies.push(pixels);
            }
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Ok(())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn readback_green(&self) -> Result<Vec<u8>, ExtendedWebGpuRendererError> {
        let padded_row_bytes = (self.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tektronix 4051 extended screen-copy readback"),
            size: u64::from(padded_row_bytes) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 extended screen-copy readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self._output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait)
            .map_err(|error| ExtendedWebGpuRendererError::Readback(error.to_string()))?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| ExtendedWebGpuRendererError::Readback(error.to_string()))?
            .map_err(|error| ExtendedWebGpuRendererError::Readback(error.to_string()))?;
        let mapped = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((self.width * self.height) as usize);
        for row in mapped
            .chunks_exact(padded_row_bytes as usize)
            .take(self.height as usize)
        {
            pixels.extend(
                row[..(self.width * 4) as usize]
                    .chunks_exact(4)
                    .map(|pixel| pixel[1]),
            );
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }
}

fn validate_curve(curve: DecayCurve) -> Result<(), ExtendedWebGpuRendererError> {
    match curve {
        DecayCurve::Linear => Ok(()),
        DecayCurve::Exponential { black_threshold }
            if (1.0 / 255.0..1.0).contains(&black_threshold) =>
        {
            Ok(())
        }
        DecayCurve::InversePower {
            power,
            black_threshold,
        } if power.is_finite()
            && (0.1..=100.0).contains(&power)
            && (1.0 / 255.0..1.0).contains(&black_threshold) =>
        {
            Ok(())
        }
        _ => Err(ExtendedWebGpuRendererError::InvalidDecayParameters),
    }
}

fn create_texture(device: &wgpu::Device, label: &str, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    source: &str,
    label: &str,
    texture_binding_count: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let mut entries = (0..texture_binding_count)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect::<Vec<_>>();
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: texture_binding_count,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &entries,
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    texture_views: &[&wgpu::TextureView],
    uniform_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let mut entries = texture_views
        .iter()
        .enumerate()
        .map(|(binding, texture_view)| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: wgpu::BindingResource::TextureView(texture_view),
        })
        .collect::<Vec<_>>();
    entries.push(wgpu::BindGroupEntry {
        binding: texture_views.len() as u32,
        resource: uniform_buffer.as_entire_binding(),
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Tektronix 4051 extended phosphor bindings"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    })
}

fn cursor_mark_segments(
    position: DisplayPoint,
    granularity: DisplayGranularity,
    height: u32,
) -> [VectorSegment; 2] {
    [
        mark_segment(position, 1, cursor_intensity(granularity), height),
        mark_segment(
            DisplayPoint {
                x: position.x + 1,
                y: position.y + 1,
            },
            1,
            cursor_intensity(granularity),
            height,
        ),
    ]
}

fn mark_segment(position: DisplayPoint, size: usize, intensity: f32, height: u32) -> VectorSegment {
    let point = canvas_position(position, height);
    VectorSegment::new(point, point, size as f32, intensity)
}

fn vector_segment(
    from: DisplayPoint,
    to: DisplayPoint,
    size: usize,
    intensity: f32,
    height: u32,
) -> VectorSegment {
    VectorSegment::new(
        canvas_position(from, height),
        canvas_position(to, height),
        size as f32,
        intensity,
    )
}

fn transform_segment(
    segment: &mut VectorSegment,
    zoom: f32,
    pan: [f32; 2],
    width: u32,
    height: u32,
) {
    let center = [width as f32 * 0.5, height as f32 * 0.5];
    for point in [&mut segment.start, &mut segment.end] {
        point[0] = center[0] + (point[0] - center[0]) * zoom + pan[0];
        point[1] = center[1] + (point[1] - center[1]) * zoom + pan[1];
    }
    segment.width *= zoom.min(1.0);
}

fn canvas_position(point: DisplayPoint, height: u32) -> [f32; 2] {
    [point.x as f32 + 0.5, height as f32 - point.y as f32 - 0.5]
}

fn stored_intensity(granularity: DisplayGranularity) -> f32 {
    match granularity {
        DisplayGranularity::Fine | DisplayGranularity::Mixed => 254.0 / 255.0,
        DisplayGranularity::Coarse => 240.0 / 255.0,
    }
}

fn cursor_intensity(granularity: DisplayGranularity) -> f32 {
    match granularity {
        DisplayGranularity::Fine | DisplayGranularity::Mixed => 239.0 / 255.0,
        DisplayGranularity::Coarse => 200.0 / 255.0,
    }
}
