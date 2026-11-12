//! Browser WebGPU device, surface, and presentation lifecycle.
//!
//! The emulation layer owns vector rasterization and its offscreen texture.
//! This adapter creates a browser-compatible device and copies that texture to
//! the canvas surface without reading the frame back to the CPU.

use std::sync::Arc;

use web_sys::HtmlCanvasElement;

use crate::emulation::{extended_webgpu::ExtendedWebGpuRenderer, webgpu::WebGpuVectorRenderer};

const PRESENT_SHADER: &str = r"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0)
var source_texture: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let coordinates = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = coordinates[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(source_texture, source_sampler, input.uv);
}
";

/// Presents the emulation-owned offscreen frame on an HTML canvas.
pub(super) struct BrowserWebGpuPresentation {
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    source_sampler: wgpu::Sampler,
    source: wgpu::BindGroup,
}

impl BrowserWebGpuPresentation {
    /// Requests a browser WebGPU adapter and creates its offscreen renderer and canvas pipeline.
    pub(super) async fn initialize(
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<(WebGpuVectorRenderer, Self), String> {
        let context = create_browser_gpu(canvas, width, height).await?;
        let renderer = WebGpuVectorRenderer::new(
            Arc::clone(&context.device),
            Arc::clone(&context.queue),
            width,
            height,
        )
        .map_err(|error| format!("could not create the vector renderer: {error}"))?;
        let presentation = Self::new(context, renderer.target_view());
        Ok((renderer, presentation))
    }

    /// Requests browser GPU resources for the opt-in phosphor-aging renderer.
    pub(super) async fn initialize_extended(
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<(ExtendedWebGpuRenderer, Self), String> {
        let context = create_browser_gpu(canvas, width, height).await?;
        let renderer = ExtendedWebGpuRenderer::new(
            Arc::clone(&context.device),
            Arc::clone(&context.queue),
            width,
            height,
        )
        .map_err(|error| format!("could not create the Extended WebGPU renderer: {error}"))?;
        let presentation = Self::new(context, renderer.target_view());
        Ok((renderer, presentation))
    }

    fn new(context: BrowserGpuContext, source_view: &wgpu::TextureView) -> Self {
        let pipeline = create_present_pipeline(&context.device, context.format);
        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Tektronix 4051 WebGPU presentation sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let source = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Tektronix 4051 WebGPU frame source"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
        let BrowserGpuContext {
            surface,
            device,
            queue,
            ..
        } = context;
        Self {
            surface,
            device,
            queue,
            pipeline,
            source_sampler: sampler,
            source,
        }
    }

    /// Rebinds the browser compositor to a renderer-owned presentation texture.
    pub(super) fn set_source_view(&mut self, source_view: &wgpu::TextureView) {
        self.source = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tektronix 4051 WebGPU frame source"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.source_sampler),
                },
            ],
        });
    }

    /// Composites the current GPU frame into the canvas surface.
    pub(super) fn present(&self) -> Result<(), String> {
        let surface_frame = self
            .surface
            .get_current_texture()
            .map_err(|error| error.to_string())?;
        let view = surface_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tektronix 4051 WebGPU canvas presentation"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tektronix 4051 WebGPU canvas presentation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.source, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        surface_frame.present();
        Ok(())
    }
}

struct BrowserGpuContext {
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    format: wgpu::TextureFormat,
}

async fn create_browser_gpu(
    canvas: HtmlCanvasElement,
    width: u32,
    height: u32,
) -> Result<BrowserGpuContext, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..Default::default()
    });
    let surface: wgpu::Surface<'static> = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|error| format!("could not create the WebGPU canvas surface: {error}"))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|error| format!("could not find a WebGPU adapter: {error}"))?;
    let adapter_info = adapter.get_info();
    if adapter_info.backend != wgpu::Backend::BrowserWebGpu {
        return Err(format!(
            "expected the browser WebGPU backend, received {:?}",
            adapter_info.backend
        ));
    }

    let capabilities = surface.get_capabilities(&adapter);
    if !capabilities
        .usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        return Err("the WebGPU canvas does not support render attachments".to_owned());
    }
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| capabilities.formats.first().copied())
        .ok_or_else(|| "the WebGPU canvas has no supported texture format".to_owned())?;
    let alpha_mode = capabilities
        .alpha_modes
        .iter()
        .copied()
        .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
        .or_else(|| capabilities.alpha_modes.first().copied())
        .ok_or_else(|| "the WebGPU canvas has no supported alpha mode".to_owned())?;
    let configuration = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: vec![],
    };
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Tektronix 4051 browser WebGPU device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
        .map_err(|error| format!("could not create the WebGPU device: {error}"))?;
    surface.configure(&device, &configuration);
    Ok(BrowserGpuContext {
        surface,
        device: Arc::new(device),
        queue: Arc::new(queue),
        format,
    })
}

fn create_present_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Tektronix 4051 WebGPU presentation shader"),
        source: wgpu::ShaderSource::Wgsl(PRESENT_SHADER.into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Tektronix 4051 WebGPU frame source"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Tektronix 4051 WebGPU presentation layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Tektronix 4051 WebGPU presentation pipeline"),
        layout: Some(&pipeline_layout),
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
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}
