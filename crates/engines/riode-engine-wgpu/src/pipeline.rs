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
/// StrokeAlign is fully resolved CPU-side: `fbox`/`radii` are the outer
/// wall (fill + border), `ibox`/`iradii` the inner wall (fill only). The
/// fragment paints fill inside the inner wall and border between walls
/// (SDF double-wall). `grad` is reserved for gradient fills
/// (dir.x, dir.y, stop0_t, stop1_t) with the mode flag in `grad.w`;
/// zeroed for solid fills.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RRectVertex {
    pub pos: [f32; 2],
    pub fbox: [f32; 4],
    pub radii: [f32; 4],
    pub ibox: [f32; 4],
    pub iradii: [f32; 4],
    pub fill: [f32; 4],
    pub stroke: [f32; 4],
    pub grad: [f32; 4],
}

impl RRectVertex {
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
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 88,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 104,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Shadow quad: device-space spread+offset box + corner radii + color
/// with layer alpha pre-baked (intensity * color.a * layer.opacity *
/// node opacity) + sigma in `params.x`.
///
/// One vertex run per shadow *layer* (multi-layer shadows emit several
/// runs) — keeps the attribute count low and the CPU order (extent
/// descending, tighter over softer) identical to the other engines.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ShadowVertex {
    pub pos: [f32; 2],
    pub fbox: [f32; 4],
    pub radii: [f32; 4],
    pub color: [f32; 4],
    pub params: [f32; 4],
}

impl ShadowVertex {
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
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

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

pub struct Pipelines {
    pub rect_pipeline: wgpu::RenderPipeline,
    /// Same shaders/geometry as rect_pipeline but blending disabled:
    /// damage-base quads replace stale pixels instead of blending over.
    pub rect_replace_pipeline: wgpu::RenderPipeline,
    pub rrect_pipeline: wgpu::RenderPipeline,
    pub shadow_pipeline: wgpu::RenderPipeline,
    pub ortho_layout: wgpu::BindGroupLayout,
    pub blit_layout: wgpu::BindGroupLayout,
    pub blit_pipeline: wgpu::RenderPipeline,
    pub glyph_pipeline: wgpu::RenderPipeline,
    pub glyph_layout: wgpu::BindGroupLayout,
    pub glyph_sampler_linear: wgpu::Sampler,
    pub glyph_sampler_nearest: wgpu::Sampler,
    pub image_pipeline: wgpu::RenderPipeline,
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

    // Rounded-rect pipeline: SDF fill disc + stroke ring (same target).
    let rrect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-rrect"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rounded.wgsl").into()),
    });
    let rrect_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-rrect-pipe-layout"),
        bind_group_layouts: &[&ortho_layout],
        push_constant_ranges: &[],
    });
    let rrect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-rrect-pipeline"),
        layout: Some(&rrect_pipe_layout),
        vertex: wgpu::VertexState {
            module: &rrect_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[RRectVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &rrect_shader,
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

    // Shadow pipeline: SDF box blurred by a 4-tap Gaussian integral
    // along the distance axis (same target, straight alpha).
    let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-shadow"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shadow.wgsl").into()),
    });
    let shadow_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-shadow-pipe-layout"),
        bind_group_layouts: &[&ortho_layout],
        push_constant_ranges: &[],
    });
    let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-shadow-pipeline"),
        layout: Some(&shadow_pipe_layout),
        vertex: wgpu::VertexState {
            module: &shadow_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[ShadowVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shadow_shader,
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

    // Blit pipeline (persistent target -> swapchain).
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
                    "SRGB_ENCODE".to_string(),
                    if swapchain_format.is_srgb() { 0.0 } else { 1.0 },
                )]),
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format: swapchain_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Glyph pipeline (coverage atlas -> persistent target).
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

    // Image pipeline: premultiplied sRGB bytes -> linear (shader un-
    // premultiplies, decodes, re-premultiplies) -> persistent target,
    // premultiplied alpha, per-image bind group on group 1 (same layout
    // shape as the glyph atlas).
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

    Pipelines {
        rect_pipeline,
        rect_replace_pipeline,
        rrect_pipeline,
        shadow_pipeline,
        ortho_layout,
        blit_layout,
        blit_pipeline,
        glyph_pipeline,
        glyph_layout,
        glyph_sampler_linear,
        glyph_sampler_nearest,
        image_pipeline,
    }
}
