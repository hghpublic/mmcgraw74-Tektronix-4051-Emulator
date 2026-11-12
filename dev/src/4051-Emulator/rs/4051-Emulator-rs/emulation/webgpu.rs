//! Offscreen WebGPU rasterization for vector segments in emulation coordinates.
//!
//! The GPU pass consumes emulation display events without owning the machine,
//! session, or window lifecycle. Hosts provide GPU resources and decide how to
//! present the resulting texture; native readback supports validation and screen
//! copies, while browser presentation can use the texture directly. Each input
//! vector stays compact on the CPU; the vertex shader expands its antialiased quad.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
use std::{collections::HashSet, error::Error, fmt, mem::size_of, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
use std::{mem::size_of_val, time::Duration};

use wgpu::util::DeviceExt;

use super::display::{DisplayEvent, DisplayEventBatch, DisplayGranularity, DisplayPoint};
use crate::machine::display::DisplayOperation;

#[cfg(not(target_arch = "wasm32"))]
const BYTES_PER_PIXEL: u32 = 4;
#[cfg(not(target_arch = "wasm32"))]
const COPY_BYTES_PER_ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
const ANTIALIAS_RADIUS: f32 = 1.0;
#[cfg(not(target_arch = "wasm32"))]
const MAX_PENDING_SCREEN_COPIES: usize = 8;
#[cfg(not(target_arch = "wasm32"))]
const MAX_PENDING_GPU_SUBMISSIONS: usize = 256;

const SHADER: &str = r"
struct RendererUniforms {
    dimensions: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> renderer: RendererUniforms;

struct VertexInput {
    @location(0) start: vec2<f32>,
    @location(1) end: vec2<f32>,
    @location(2) direction: vec2<f32>,
    @location(3) width: f32,
    @location(4) intensity: f32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) start: vec2<f32>,
    @location(1) @interpolate(flat) end: vec2<f32>,
    @location(2) @interpolate(flat) width: f32,
    @location(3) @interpolate(flat) intensity: f32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    input: VertexInput,
) -> VertexOutput {
    let normal = vec2<f32>(-input.direction.y, input.direction.x);
    let radius = input.width * 0.5 + 1.0;
    let extended_start = input.start - input.direction * radius;
    let extended_end = input.end + input.direction * radius;
    let corners = array<vec2<f32>, 4>(
        extended_start - normal * radius,
        extended_start + normal * radius,
        extended_end + normal * radius,
        extended_end - normal * radius,
    );
    let corner_order = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let position = corners[corner_order[vertex_index]];

    var output: VertexOutput;
    output.position = vec4<f32>(
        position.x / renderer.dimensions.x * 2.0 - 1.0,
        1.0 - position.y / renderer.dimensions.y * 2.0,
        0.0,
        1.0,
    );
    output.start = input.start;
    output.end = input.end;
    output.width = input.width;
    output.intensity = input.intensity;
    return output;
}

@fragment
fn fs_main(
    input: VertexOutput,
) -> @location(0) vec4<f32> {
    let delta = input.end - input.start;
    let length_squared = dot(delta, delta);
    var nearest = input.start;
    if length_squared > 0.000001 {
        let along = clamp(dot(input.position.xy - input.start, delta) / length_squared, 0.0, 1.0);
        nearest = input.start + along * delta;
    }

    let distance = length(input.position.xy - nearest);
    let half_width = input.width * 0.5;
    let coverage = 1.0 - smoothstep(max(half_width - 0.5, 0.0), half_width + 0.5, distance);
    let phosphor = input.intensity * coverage;
    return vec4<f32>(0.0, phosphor, 0.0, phosphor);
}
";

/// One green-phosphor vector segment in top-left-origin canvas coordinates.
///
/// Width is measured in logical output pixels. Intensity is normalized to the
/// inclusive range 0.0 through 1.0. Invalid values are reported by
/// [`WebGpuVectorRenderer::render_segments`] before GPU submission.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorSegment {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub width: f32,
    pub intensity: f32,
}

impl VectorSegment {
    /// Creates a vector segment; validation occurs when it is submitted.
    pub const fn new(start: [f32; 2], end: [f32; 2], width: f32, intensity: f32) -> Self {
        Self {
            start,
            end,
            width,
            intensity,
        }
    }
}

/// Failure while creating or using the offscreen WebGPU vector renderer.
#[derive(Debug, PartialEq, Eq)]
pub enum WebGpuRendererError {
    InvalidDimensions {
        width: u32,
        height: u32,
    },
    DimensionsExceedDevice {
        width: u32,
        height: u32,
        maximum: u32,
    },
    InvalidSegment {
        index: usize,
    },
    DisplayEventsDropped,
    TooManyVertices,
    PendingSubmissionsFull,
    SubmissionQueuePoisoned,
    Readback(String),
}

impl fmt::Display for WebGpuRendererError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(
                    formatter,
                    "render target dimensions must be nonzero, got {width}x{height}"
                )
            }
            Self::DimensionsExceedDevice {
                width,
                height,
                maximum,
            } => write!(
                formatter,
                "render target {width}x{height} exceeds device limit {maximum}"
            ),
            Self::InvalidSegment { index } => write!(
                formatter,
                "vector segment {index} has non-finite coordinates, a non-positive width, or invalid intensity"
            ),
            Self::DisplayEventsDropped => {
                write!(
                    formatter,
                    "display event history overflowed before GPU rendering"
                )
            }
            Self::TooManyVertices => write!(formatter, "vector batch exceeds the GPU draw limit"),
            Self::PendingSubmissionsFull => {
                write!(formatter, "pending WebGPU submission queue is full")
            }
            Self::SubmissionQueuePoisoned => {
                write!(formatter, "pending WebGPU submission queue is unavailable")
            }
            Self::Readback(error) => write!(formatter, "WebGPU frame readback failed: {error}"),
        }
    }
}

impl Error for WebGpuRendererError {}

/// Keeps worker-generated command buffers ordered until the native UI submits them.
#[cfg(not(target_arch = "wasm32"))]
pub struct DeferredWebGpuSubmissions {
    queue: Arc<wgpu::Queue>,
    pending: Mutex<Vec<wgpu::CommandBuffer>>,
    submit_lock: Mutex<()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl DeferredWebGpuSubmissions {
    /// Creates a deferred submission queue backed by the host's WGPU queue.
    pub fn new(queue: Arc<wgpu::Queue>) -> Self {
        Self {
            queue,
            pending: Mutex::new(Vec::new()),
            submit_lock: Mutex::new(()),
        }
    }

    fn enqueue(&self, command_buffer: wgpu::CommandBuffer) -> Result<(), WebGpuRendererError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| WebGpuRendererError::SubmissionQueuePoisoned)?;
        if pending.len() == MAX_PENDING_GPU_SUBMISSIONS {
            return Err(WebGpuRendererError::PendingSubmissionsFull);
        }
        pending.push(command_buffer);
        Ok(())
    }

    /// Submits all queued command buffers in their original order.
    pub fn submit_pending(&self) -> Result<(), WebGpuRendererError> {
        let _submit_guard = self
            .submit_lock
            .lock()
            .map_err(|_| WebGpuRendererError::SubmissionQueuePoisoned)?;
        let pending = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| WebGpuRendererError::SubmissionQueuePoisoned)?;
            std::mem::take(&mut *pending)
        };
        if !pending.is_empty() {
            self.queue.submit(pending);
        }
        Ok(())
    }
}

// One instance per vector avoids uploading six expanded vertices for each segment.
#[repr(C)]
#[derive(Clone, Copy)]
struct SegmentInstance {
    start: [f32; 2],
    end: [f32; 2],
    direction: [f32; 2],
    width: f32,
    intensity: f32,
}

/// Stores transient geometry once in the packed form reused for redraws.
#[derive(Default)]
struct TransientGeometry {
    segment_count: usize,
    instance_bytes: Vec<u8>,
    // Max blending makes identical cursor marks idempotent until transient geometry clears.
    cursor_mark_keys: HashSet<(DisplayPoint, u32)>,
}

impl TransientGeometry {
    fn append(&mut self, segments: &[VectorSegment]) -> Result<(), WebGpuRendererError> {
        let next_count = self
            .segment_count
            .checked_add(segments.len())
            .ok_or(WebGpuRendererError::TooManyVertices)?;
        checked_segment_instance_count(next_count)?;
        append_transient_segment_bytes(segments, self.segment_count, &mut self.instance_bytes)?;
        self.segment_count = next_count;
        Ok(())
    }

    fn clear(&mut self) {
        self.segment_count = 0;
        self.instance_bytes.clear();
        self.cursor_mark_keys.clear();
    }

    fn refresh(&mut self) -> bool {
        if self.segment_count == 0 {
            return false;
        }
        self.clear();
        true
    }
}

/// Holds retained strokes and refreshable cursor marks until GPU staging succeeds.
#[derive(Default)]
struct PendingDisplayGeometry {
    stored_segments: Vec<VectorSegment>,
    transient_segments: Vec<VectorSegment>,
    transient_cursor_mark_keys: Vec<(DisplayPoint, u32)>,
    erase_flash: bool,
    has_events: bool,
}

impl PendingDisplayGeometry {
    fn stage_persistent_vector(&mut self, segment: VectorSegment) {
        self.stored_segments.push(segment);
        self.has_events = true;
    }

    fn stage_cursor_mark(
        &mut self,
        position: DisplayPoint,
        granularity: DisplayGranularity,
        height: u32,
        transient_geometry: &TransientGeometry,
    ) {
        let intensity = cursor_intensity(granularity);
        let key = (position, intensity.to_bits());
        if transient_geometry.cursor_mark_keys.contains(&key)
            || self.transient_cursor_mark_keys.contains(&key)
        {
            return;
        }

        self.transient_segments
            .extend(cursor_mark_segments(position, granularity, height));
        self.transient_cursor_mark_keys.push(key);
        self.has_events = true;
    }

    fn commit(
        self,
        transient_geometry: &mut TransientGeometry,
        pending_stored_segments: &mut Vec<VectorSegment>,
        erase_flash: &mut bool,
        display_dirty: &mut bool,
    ) -> Result<(), WebGpuRendererError> {
        transient_geometry.append(&self.transient_segments)?;
        transient_geometry
            .cursor_mark_keys
            .extend(self.transient_cursor_mark_keys);
        pending_stored_segments.extend(self.stored_segments);
        *erase_flash |= self.erase_flash;
        *display_dirty |= self.has_events;
        Ok(())
    }
}

/// A GPU-backed offscreen vector rasterizer in the emulation layer.
///
/// Each render clears the offscreen target and draws the supplied vector batch.
/// Persistent phosphor state, machine-operation decoding, and screen-copy
/// lifecycle remain owned by the later renderer/session integration.
pub struct WebGpuVectorRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::BindGroup,
    persistent_texture: wgpu::Texture,
    persistent_view: wgpu::TextureView,
    pending_stored_segments: Vec<VectorSegment>,
    transient_geometry: TransientGeometry,
    erase_flash: bool,
    persistent_clear: bool,
    display_dirty: bool,
    revision: u64,
    screen_copies: Vec<Vec<u8>>,
    dropped_screen_copies: usize,
    #[cfg(not(target_arch = "wasm32"))]
    deferred_submissions: Option<Arc<DeferredWebGpuSubmissions>>,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl WebGpuVectorRenderer {
    /// Creates an offscreen vector target using platform-provided GPU resources.
    ///
    /// The native or browser host selects the adapter and creates the device;
    /// this emulation-owned renderer creates only its pipeline and frame target.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
    ) -> Result<Self, WebGpuRendererError> {
        validate_dimensions(width, height, device.limits().max_texture_dimension_2d)?;

        let mut uniform_bytes = Vec::with_capacity(2 * size_of::<f32>());
        uniform_bytes.extend_from_slice(&(width as f32).to_ne_bytes());
        uniform_bytes.extend_from_slice(&(height as f32).to_ne_bytes());
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tektronix 4051 vector dimensions"),
            contents: &uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let pipeline = create_pipeline(&device);
        let uniforms = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tektronix 4051 vector dimensions"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tektronix 4051 offscreen vector frame"),
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
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let persistent_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tektronix 4051 persistent vector layer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let persistent_view =
            persistent_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut renderer = Self {
            device,
            queue,
            pipeline,
            uniforms,
            persistent_texture,
            persistent_view,
            pending_stored_segments: Vec::new(),
            transient_geometry: TransientGeometry::default(),
            erase_flash: false,
            persistent_clear: false,
            display_dirty: false,
            revision: 0,
            screen_copies: Vec::new(),
            dropped_screen_copies: 0,
            #[cfg(not(target_arch = "wasm32"))]
            deferred_submissions: None,
            target,
            target_view,
            width,
            height,
        };
        renderer.clear_persistent_layer()?;
        renderer.compose_display()?;
        Ok(renderer)
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Defers display-batch submissions until the host drains them before presentation.
    pub fn set_deferred_submissions(&mut self, submissions: Arc<DeferredWebGpuSubmissions>) {
        self.deferred_submissions = Some(submissions);
    }

    /// Returns the offscreen frame dimensions.
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Returns the texture view for host presentation or native texture registration.
    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.target_view
    }

    /// Returns the renderer's wrapping presentation revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Clears the target and rasterizes one ordered batch of vector segments.
    ///
    /// Overlapping segments retain the greatest coverage/intensity. Validation
    /// failures are returned before any GPU work is submitted.
    pub fn render_segments(
        &mut self,
        segments: &[VectorSegment],
    ) -> Result<(), WebGpuRendererError> {
        self.draw_segments(&self.target_view, segments, Some(wgpu::Color::TRANSPARENT))?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub(crate) fn apply_display_operation(
        &mut self,
        operation: DisplayOperation,
        events: DisplayEventBatch,
        granularity: DisplayGranularity,
    ) -> Result<(), WebGpuRendererError> {
        if events.dropped {
            return Err(WebGpuRendererError::DisplayEventsDropped);
        }

        let mut pending = PendingDisplayGeometry::default();
        for event in events.events {
            match event {
                DisplayEvent::TransientCursorMark(position) => {
                    pending.stage_cursor_mark(
                        position,
                        granularity,
                        self.height,
                        &self.transient_geometry,
                    );
                }
                DisplayEvent::StoredMark { position, size } => {
                    pending.has_events = true;
                    pending.stored_segments.push(mark_segment(
                        position,
                        size,
                        stored_intensity(granularity),
                        self.height,
                    ));
                }
                DisplayEvent::TransientVector { from, to } => {
                    pending.stage_persistent_vector(vector_segment(
                        from,
                        to,
                        1,
                        stored_intensity(granularity),
                        self.height,
                    ));
                }
                DisplayEvent::StoredVector { from, to, size } => {
                    pending.stage_persistent_vector(vector_segment(
                        from,
                        to,
                        size,
                        stored_intensity(granularity),
                        self.height,
                    ));
                }
                DisplayEvent::Erase => {
                    pending.erase_flash = true;
                    pending.has_events = true;
                }
            }
        }

        pending.commit(
            &mut self.transient_geometry,
            &mut self.pending_stored_segments,
            &mut self.erase_flash,
            &mut self.display_dirty,
        )?;

        match operation {
            DisplayOperation::BeginErase => {
                // Clear prior phosphor now so later segments survive the delayed completion.
                self.pending_stored_segments.clear();
                self.transient_geometry.clear();
                self.persistent_clear = true;
                self.erase_flash = true;
                self.display_dirty = true;
            }
            DisplayOperation::CompleteErase => {
                self.erase_flash = false;
                self.display_dirty = true;
            }
            DisplayOperation::ScreenCopy => {
                self.finish_display_batch()?;
                self.capture_screen_copy()?;
                return Ok(());
            }
            DisplayOperation::ScreenCommand { .. } => {}
        }
        Ok(())
    }

    /// Flushes staged geometry before presentation or an event-time screen capture.
    pub(crate) fn finish_display_batch(&mut self) -> Result<(), WebGpuRendererError> {
        if !self.display_dirty {
            return Ok(());
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 display operation batch"),
            });
        if self.persistent_clear || !self.pending_stored_segments.is_empty() {
            self.record_segments(
                &mut encoder,
                &self.persistent_view,
                &self.pending_stored_segments,
                self.persistent_clear.then_some(wgpu::Color::TRANSPARENT),
            )?;
        }
        self.record_display_composition(&mut encoder)?;
        let command_buffer = encoder.finish();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(submissions) = &self.deferred_submissions {
            submissions.enqueue(command_buffer)?;
        } else {
            self.queue.submit(Some(command_buffer));
        }
        #[cfg(target_arch = "wasm32")]
        self.queue.submit(Some(command_buffer));
        self.pending_stored_segments.clear();
        self.persistent_clear = false;
        self.revision = self.revision.wrapping_add(1);
        self.display_dirty = false;
        Ok(())
    }

    pub(crate) fn refresh_display(&mut self) -> Result<(), WebGpuRendererError> {
        if self.transient_geometry.refresh() {
            self.display_dirty = true;
        }
        self.finish_display_batch()
    }

    pub(crate) fn take_screen_copies(&mut self) -> (Vec<Vec<u8>>, usize) {
        (
            std::mem::take(&mut self.screen_copies),
            std::mem::take(&mut self.dropped_screen_copies),
        )
    }

    fn capture_screen_copy(&mut self) -> Result<(), WebGpuRendererError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(submissions) = &self.deferred_submissions {
                submissions.submit_pending()?;
            }
            let rgba = self.readback_rgba()?;
            if self.screen_copies.len() == MAX_PENDING_SCREEN_COPIES {
                self.dropped_screen_copies = self.dropped_screen_copies.saturating_add(1);
                return Ok(());
            }
            self.screen_copies
                .push(rgba.chunks_exact(4).map(|pixel| pixel[1]).collect());
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err(WebGpuRendererError::Readback(
                "synchronous screen-copy readback is not available in the browser".to_owned(),
            ))
        }
    }

    fn clear_persistent_layer(&self) -> Result<(), WebGpuRendererError> {
        self.draw_segments(&self.persistent_view, &[], Some(wgpu::Color::TRANSPARENT))
    }

    fn compose_display(&mut self) -> Result<(), WebGpuRendererError> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 compose vector frame"),
            });
        self.record_display_composition(&mut encoder)?;
        self.queue.submit(Some(encoder.finish()));
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn record_display_composition(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), WebGpuRendererError> {
        let transient_instance_count =
            checked_segment_instance_count(self.transient_geometry.segment_count)?;
        if self.erase_flash {
            self.record_prepared_segments(
                encoder,
                &self.target_view,
                &self.transient_geometry.instance_bytes,
                transient_instance_count,
                Some(wgpu::Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                }),
            )?;
        } else {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.persistent_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
            self.record_prepared_segments(
                encoder,
                &self.target_view,
                &self.transient_geometry.instance_bytes,
                transient_instance_count,
                None,
            )?;
        }
        Ok(())
    }

    fn draw_segments(
        &self,
        view: &wgpu::TextureView,
        segments: &[VectorSegment],
        clear: Option<wgpu::Color>,
    ) -> Result<(), WebGpuRendererError> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 vector frame"),
            });
        self.record_segments(&mut encoder, view, segments, clear)?;
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn record_segments(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        segments: &[VectorSegment],
        clear: Option<wgpu::Color>,
    ) -> Result<(), WebGpuRendererError> {
        let instance_count = checked_segment_instance_count(segments.len())?;
        let instances = build_segment_instances(segments)?;
        let bytes = segment_instance_bytes(&instances);
        self.record_prepared_segments(encoder, view, &bytes, instance_count, clear)
    }

    fn record_prepared_segments(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        instance_bytes: &[u8],
        instance_count: u32,
        clear: Option<wgpu::Color>,
    ) -> Result<(), WebGpuRendererError> {
        if instance_count == 0 && clear.is_none() {
            return Ok(());
        }
        let vertex_buffer = if instance_count == 0 {
            None
        } else {
            Some(
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Tektronix 4051 vector segment instances"),
                        contents: instance_bytes,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };
        {
            let load = clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tektronix 4051 antialiased vector pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniforms, &[]);
            if let Some(vertex_buffer) = &vertex_buffer {
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..6, 0..instance_count);
            }
        }
        Ok(())
    }

    /// Reads the offscreen RGBA frame for native validation.
    ///
    /// This blocking helper is native-only. Browser presentation must use the
    /// GPU texture directly and perform readback only for a requested capture.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn readback_rgba(&self) -> Result<Vec<u8>, WebGpuRendererError> {
        let unpadded_bytes_per_row =
            self.width
                .checked_mul(BYTES_PER_PIXEL)
                .ok_or(WebGpuRendererError::Readback(
                    "row byte count overflow".to_owned(),
                ))?;
        let padded_bytes_per_row = unpadded_bytes_per_row
            .div_ceil(COPY_BYTES_PER_ROW_ALIGNMENT)
            .checked_mul(COPY_BYTES_PER_ROW_ALIGNMENT)
            .ok_or(WebGpuRendererError::Readback(
                "padded row byte count overflow".to_owned(),
            ))?;
        let buffer_size = u64::from(padded_bytes_per_row)
            .checked_mul(u64::from(self.height))
            .ok_or(WebGpuRendererError::Readback(
                "buffer size overflow".to_owned(),
            ))?;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tektronix 4051 native frame readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 native frame readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::WaitForSubmissionIndex(submission))
            .map_err(|error| WebGpuRendererError::Readback(error.to_string()))?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| WebGpuRendererError::Readback(error.to_string()))?
            .map_err(|error| WebGpuRendererError::Readback(error.to_string()))?;

        let mapped = slice.get_mapped_range();
        let row_bytes = unpadded_bytes_per_row as usize;
        let padded_row_bytes = padded_bytes_per_row as usize;
        let mut pixels = Vec::with_capacity(row_bytes * self.height as usize);
        for row in 0..self.height as usize {
            let start = row * padded_row_bytes;
            pixels.extend_from_slice(&mapped[start..start + row_bytes]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }
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

fn validate_dimensions(width: u32, height: u32, maximum: u32) -> Result<(), WebGpuRendererError> {
    if width == 0 || height == 0 {
        return Err(WebGpuRendererError::InvalidDimensions { width, height });
    }
    if width > maximum || height > maximum {
        return Err(WebGpuRendererError::DimensionsExceedDevice {
            width,
            height,
            maximum,
        });
    }
    Ok(())
}

fn create_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32,
        4 => Float32
    ];
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Tektronix 4051 antialiased vector shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<SegmentInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    };
    let max_blend = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Tektronix 4051 vector pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout],
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
                blend: Some(wgpu::BlendState {
                    color: max_blend,
                    alpha: max_blend,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

fn build_segment_instances(
    segments: &[VectorSegment],
) -> Result<Vec<SegmentInstance>, WebGpuRendererError> {
    let mut instances = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let direction = validate_segment(*segment, index)?;
        instances.push(SegmentInstance {
            start: segment.start,
            end: segment.end,
            direction,
            width: segment.width,
            intensity: segment.intensity,
        });
    }
    Ok(instances)
}

fn checked_segment_instance_count(segment_count: usize) -> Result<u32, WebGpuRendererError> {
    let vertex_count = segment_count
        .checked_mul(6)
        .and_then(|count| u32::try_from(count).ok())
        .ok_or(WebGpuRendererError::TooManyVertices)?;
    Ok(vertex_count / 6)
}

fn append_transient_segment_bytes(
    segments: &[VectorSegment],
    first_index: usize,
    bytes: &mut Vec<u8>,
) -> Result<(), WebGpuRendererError> {
    first_index
        .checked_add(segments.len())
        .ok_or(WebGpuRendererError::TooManyVertices)?;
    let original_len = bytes.len();
    for (offset, segment) in segments.iter().copied().enumerate() {
        let index = first_index + offset;
        let direction = match validate_segment(segment, index) {
            Ok(direction) => direction,
            Err(error) => {
                bytes.truncate(original_len);
                return Err(error);
            }
        };
        append_segment_instance_bytes(
            SegmentInstance {
                start: segment.start,
                end: segment.end,
                direction,
                width: segment.width,
                intensity: segment.intensity,
            },
            bytes,
        );
    }
    Ok(())
}

fn validate_segment(segment: VectorSegment, index: usize) -> Result<[f32; 2], WebGpuRendererError> {
    let is_finite = segment
        .start
        .into_iter()
        .chain(segment.end)
        .chain([segment.width, segment.intensity])
        .all(f32::is_finite);
    if !is_finite || segment.width <= 0.0 || !(0.0..=1.0).contains(&segment.intensity) {
        return Err(WebGpuRendererError::InvalidSegment { index });
    }

    let delta = [
        segment.end[0] - segment.start[0],
        segment.end[1] - segment.start[1],
    ];
    if !delta.into_iter().all(f32::is_finite) {
        return Err(WebGpuRendererError::InvalidSegment { index });
    }
    let length = delta[0].hypot(delta[1]);
    if !length.is_finite() {
        return Err(WebGpuRendererError::InvalidSegment { index });
    }
    let direction = if length > f32::EPSILON {
        [delta[0] / length, delta[1] / length]
    } else {
        [1.0, 0.0]
    };
    let normal = [-direction[1], direction[0]];
    let radius = segment.width * 0.5 + ANTIALIAS_RADIUS;
    let extended_start = [
        segment.start[0] - direction[0] * radius,
        segment.start[1] - direction[1] * radius,
    ];
    let extended_end = [
        segment.end[0] + direction[0] * radius,
        segment.end[1] + direction[1] * radius,
    ];
    let corners = [
        [
            extended_start[0] - normal[0] * radius,
            extended_start[1] - normal[1] * radius,
        ],
        [
            extended_start[0] + normal[0] * radius,
            extended_start[1] + normal[1] * radius,
        ],
        [
            extended_end[0] + normal[0] * radius,
            extended_end[1] + normal[1] * radius,
        ],
        [
            extended_end[0] - normal[0] * radius,
            extended_end[1] - normal[1] * radius,
        ],
    ];
    if !corners
        .iter()
        .flatten()
        .all(|coordinate| coordinate.is_finite())
    {
        return Err(WebGpuRendererError::InvalidSegment { index });
    }
    Ok(direction)
}

fn segment_instance_bytes(instances: &[SegmentInstance]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(size_of_val(instances));
    for instance in instances {
        append_segment_instance_bytes(*instance, &mut bytes);
    }
    bytes
}

fn append_segment_instance_bytes(instance: SegmentInstance, bytes: &mut Vec<u8>) {
    for value in instance
        .start
        .into_iter()
        .chain(instance.end)
        .chain(instance.direction)
        .chain([instance.width, instance.intensity])
    {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
}

#[cfg(test)]
#[path = "webgpu/tests.rs"]
mod tests;
