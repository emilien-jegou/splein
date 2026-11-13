// Single responsibility: Rect + glyph + blit pipeline creation and vertex staging.

use bytemuck::{Pod, Zeroable};

/// CPU-transformed rect vertex: screen-space pixels + straight-alpha color.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RectVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

impl RectVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// CPU-transformed glyph vertex: screen-space pixels + atlas UV + color.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GlyphVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl GlyphVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Rounded-rect vertex: device-space outer box + inner (fill) box +
/// corner radii for both walls + fill / stroke colors + gradient slot.
///
/// Textured image quad: device-space corners + full-texture UV + tint
/// (straight-alpha, premultiplied in the shader) + current opacity in
/// `color.a`. Full-quad UVs let the hardware pick the mip level from
/// screen-space derivatives.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ImageVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl ImageVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Row-major 4×5 CSS color matrix removed with the legacy filter path:
/// layer compositing is a plain texture blit now.




pub struct Pipelines {
    pub rect_pipeline: wgpu::RenderPipeline,
    /// Same shaders/geometry as rect_pipeline but blending disabled:
    /// damage-base quads replace stale pixels instead of blending over.
    pub rect_replace_pipeline: wgpu::RenderPipeline,
    pub ortho_layout: wgpu::BindGroupLayout,
    pub blit_layout: wgpu::BindGroupLayout,
    pub blit_pipeline: wgpu::RenderPipeline,
    pub glyph_pipeline: wgpu::RenderPipeline,
    pub glyph_layout: wgpu::BindGroupLayout,
    pub glyph_sampler_linear: wgpu::Sampler,
    pub glyph_sampler_nearest: wgpu::Sampler,
    pub image_pipeline: wgpu::RenderPipeline,
    /// Layer-composite layout: the layer source texture (textureLoad,
    /// no sampler — blit precedent), with ortho in group 0 like every
    /// scene pipeline.
    pub filter_layout: wgpu::BindGroupLayout,
    /// fullscreen triangle, `textureLoad` tap, blend disabled (each
    /// target is cleared by its load op): src-over of the layer
    /// offscreen onto the parent. Premultiplied blending — scene
    /// targets accumulate premultiplied by construction.
    pub composite_pipeline: wgpu::RenderPipeline,
}

fn ortho_bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-ortho-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::VERTEX,
            binding: 0,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

pub(crate) fn glyph_bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-glyph-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 0,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 1,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub fn create_pipelines(
    device: &wgpu::Device,
    target_format: wgpu::TextureFormat,
    swapchain_format: wgpu::TextureFormat,
) -> Pipelines {
    // Rect pipeline (renders into Rgba8Unorm persistent target).
    let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-rect"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rect.wgsl").into()),
    });
    let ortho_layout = ortho_bind_layout(device);
    let rect_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-rect-pipe-layout"),
        bind_group_layouts: &[&ortho_layout],
        push_constant_ranges: &[],
    });
    let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-rect-pipeline"),
        layout: Some(&rect_pipe_layout),
        vertex: wgpu::VertexState {
            module: &rect_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[RectVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &rect_shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Replace pipeline: identical geometry/shaders, blending disabled.
    let rect_replace_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-rect-replace-pipeline"),
        layout: Some(&rect_pipe_layout),
        vertex: wgpu::VertexState {
            module: &rect_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[RectVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &rect_shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-blit"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/solid.wgsl").into()),
    });
    let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-blit-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::FRAGMENT,
            binding: 0,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let blit_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-blit-pipe-layout"),
        bind_group_layouts: &[&blit_layout],
        push_constant_ranges: &[],
    });
    let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-blit-pipeline"),
        layout: Some(&blit_pipe_layout),
        vertex: wgpu::VertexState {
            module: &blit_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &blit_shader,
            entry_point: "fs_main",
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &std::collections::HashMap::from([(
                    "SRGB_DECODE".to_string(),
                    if swapchain_format.is_srgb() { 1.0 } else { 0.0 },
                )]),
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format: swapchain_format,
                // Premultiplied blend: the shader output is already
                // premultiplied RGB + straight alpha. Opaque windows are
                // pixel-identical to the old replace (srcA = 1); on a
                // transparent window the alpha channel reaches the compositor.
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Glyph pipeline (coverage atlas -> persistent target).
    //
    // Display correction is always on: the dwrite/GPUI contrast+gamma chain
    // (brightness-dependent contrast boost + quadratic gamma-1.8 alpha
    // correction) is the only glyph shader. The CPU twin (text::gamma)
    // applies the same chain for CPU-side alpha tables.
    let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-glyph"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/glyph.wgsl").into()),
    });
    let glyph_layout = glyph_bind_layout(device);
    let glyph_sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("riode-wgpu-glyph-sampler-linear"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let glyph_sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("riode-wgpu-glyph-sampler-nearest"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let glyph_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-glyph-pipe-layout"),
        bind_group_layouts: &[&ortho_layout, &glyph_layout],
        push_constant_ranges: &[],
    });
    let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-glyph-pipeline"),
        layout: Some(&glyph_pipe_layout),
        vertex: wgpu::VertexState {
            module: &glyph_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[GlyphVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &glyph_shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Image pipeline: premultiplied sRGB bytes composited as-is onto the
    // plain Unorm target, premultiplied alpha, per-image bind group on
    // group 1 (same layout shape as the glyph atlas).

    let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-image"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
    });
    let image_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-image-pipe-layout"),
        bind_group_layouts: &[&ortho_layout, &glyph_layout],
        push_constant_ranges: &[],
    });
    let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-image-pipeline"),
        layout: Some(&image_pipe_layout),
        vertex: wgpu::VertexState {
            module: &image_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[ImageVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &image_shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Composite: layer source over the parent target (both render into
    // the Rgba8Unorm scene targets).
    let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-composite"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/composite.wgsl").into()),
    });
    let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-filter-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::FRAGMENT,
            binding: 0,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let composite_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-composite-pipe-layout"),
        bind_group_layouts: &[&ortho_layout, &filter_layout],
        push_constant_ranges: &[],
    });
    let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-composite-pipeline"),
        layout: Some(&composite_pipe_layout),
        vertex: wgpu::VertexState {
            module: &composite_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[RectVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &composite_shader,
            entry_point: "fs_main",
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    Pipelines {
        rect_pipeline,
        rect_replace_pipeline,
        ortho_layout,
        blit_layout,
        blit_pipeline,
        glyph_pipeline,
        glyph_layout,
        glyph_sampler_linear,
        glyph_sampler_nearest,
        image_pipeline,
        filter_layout,
        composite_pipeline,
    }
}
