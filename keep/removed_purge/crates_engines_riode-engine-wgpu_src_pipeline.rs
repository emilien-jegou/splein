// Single responsibility: Rect + glyph + blit pipeline creation and vertex staging.

use bytemuck::{Pod, Zeroable};
use riode::foundation::effect::shaders::{
    BLEND_COMPOSITE_COMBINED_WGSL as BLEND_COMPOSITE_WGSL,
    BLUR_WGSL,
    COMPOSITE_WGSL as COMPOSITE_SRC,
};

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
/// node opacity) + sigma in `params.x` + inset flag in `params.y` +
/// the un-offset un-spread original box in `obox` (interior mask for
/// inset shadows; zeroed for drop shadows, which never read it).
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
    pub obox: [f32; 4],
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
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 5,
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

/// Per-pass separable-blur params (16-byte uniform): Gaussian sigma in
/// device texels plus the unit direction of this pass (one axis at a
/// time; H then V composes the 2D Gaussian). `pad` rounds the struct to
/// a full vec4.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct BlurGpu {
    pub sigma: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub pad: f32,
}

/// Row-major 4×5 CSS color matrix packed as the shader's five vec4 rows:
/// four coefficient rows (stride 16) + the offset column. A plain f32
/// array would violate the uniform address space's 16-byte stride rule.
/// `tex_off` shifts the source texel lookup away from the fragment
/// position: the alpha-shadow composite draws its quad at the shadow's
/// offset while still sampling the un-shifted layer texels (zero for
/// every normal content composite).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ColorMatrixGpu {
    pub r0: [f32; 4],
    pub r1: [f32; 4],
    pub r2: [f32; 4],
    pub r3: [f32; 4],
    pub off: [f32; 4],
    pub tex_off: [f32; 4],
}

impl ColorMatrixGpu {
    /// Packs `[r0 m0..m3 | r1 | r2 | r3]` (row-major, offset last in
    /// each row) into vec4 rows. `tex_off` starts at zero (identity
    /// sampling); alpha-shadow materials patch it after packing.
    pub fn pack(m: &[f32; 20]) -> Self {
        Self {
            r0: [m[0], m[1], m[2], m[3]],
            r1: [m[5], m[6], m[7], m[8]],
            r2: [m[10], m[11], m[12], m[13]],
            r3: [m[15], m[16], m[17], m[18]],
            off: [m[4], m[9], m[14], m[19]],
            tex_off: [0.0; 4],
        }
    }
}



/// One liquid-glass layer's frame uniforms: an engine-owned header
/// (core pre-blur sigmas, target size) followed by the floats
/// `pack_glass_uniforms` produces, laid out as `GLASS_VEC4S` `vec4`s.
///

/// Generic 256 B material pass uniform block
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct MaterialUniformsGpu {
    pub block: [f32; 64],
}

#[derive(Clone)]
pub struct MaterialPassGpu {
    pub shader: &'static str,
    pub uniforms: MaterialUniformsGpu,
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
    /// Shared material layout for the element-filter passes: source
    /// texture (textureLoad, no sampler — blit precedent) + a params
    /// uniform. Group 0 of the blur pipeline, group 1 of the composite
    /// (which puts ortho in group 0, like every scene pipeline).
    pub filter_layout: wgpu::BindGroupLayout,
    /// Separable Gaussian blur pass over a filter-layer offscreen:
    /// fullscreen triangle, nine `textureLoad` taps, blend disabled
    /// (each target is cleared by its load op).
    pub blur_pipeline: wgpu::RenderPipeline,
    /// Composites a (possibly blurred) filter-layer offscreen onto its
    /// parent: samples 1:1 by position, unpremultiplies, applies the
    /// 4×5 matrix, re-premultiplies. Premultiplied blending — scene
    /// targets accumulate premultiplied by construction (alpha blending
    /// over a transparent clear).
    pub composite_pipeline: wgpu::RenderPipeline,
    /// Backdrop-blend composite: `BLEND_COMPOSITE_WGSL` (the blend
    /// library + `composite.wgsl`'s path extended with the BeginLayer
    /// parent snapshot), group 1 = source texture + snapshot texture +
    /// params uniform. Blend disabled — the shader emits the full CSS
    /// compositing result (backdrop contribution included); hardware
    /// source-over on top would double-blend. Only blend-mode layers
    /// route here; `Normal` keeps `composite_pipeline` untouched.
    pub blend_layout: wgpu::BindGroupLayout,
    pub blend_composite_pipeline: wgpu::RenderPipeline,
    /// Shared bind-group layout + sampler for custom (material-DSL)
    /// stages. Every custom stage uses the same binding numbering (the
    /// engine entry scaffold fixes it: 0 sampler, 1 uniforms, 2 input,
    /// 3 backdrop, 4 sharp), so one layout serves them all; the
    /// *pipeline* is per-source and cached on the renderer.
    pub custom_layout: wgpu::BindGroupLayout,
    pub custom_sampler: wgpu::Sampler,
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
        source: wgpu::ShaderSource::Wgsl(riode::foundation::effect::shaders::SHADOW_WGSL.into()),
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
    // Display correction is opt-in (dwrite/GPUI experiment): only with
    // RIODE_TEXT_CORRECTION=on in the environment is the contrast+gamma
    // shader selected; default is the passthrough shader (straight
    // coverage × alpha, vello_cpu-like). No rebuild needed to flip, just
    // restart. The CPU twin (text::gamma) honors the same variable, so
    // both engines move together.
    let glyph_src: &str = if *riode::text::gamma::CORRECTION_ON {
        include_str!("shaders/glyph.wgsl")
    } else {
        include_str!("shaders/glyph_plain.wgsl")
    };
    let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-glyph"),
        source: wgpu::ShaderSource::Wgsl(glyph_src.into()),
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

    // Element-filter layer passes: separable blur + composite onto the
    // parent target (both render into the Rgba8Unorm scene targets).
    let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-blur"),
        source: wgpu::ShaderSource::Wgsl(BLUR_WGSL.into()),
    });
    let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-composite"),
        source: wgpu::ShaderSource::Wgsl(COMPOSITE_SRC.into()),
    });
    let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-filter-layout"),
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
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let blur_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-blur-pipe-layout"),
        bind_group_layouts: &[&filter_layout],
        push_constant_ranges: &[],
    });
    let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-wgpu-blur-pipeline"),
        layout: Some(&blur_pipe_layout),
        vertex: wgpu::VertexState {
            module: &blur_shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &blur_shader,
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

    // Backdrop-blend layer composite: the library + dispatcher prepended
    // to an extended copy of the composite fragment (two textures: the
    // layer source and the BeginLayer parent snapshot). Its own bind
    // group layout — `filter_layout` stays two-entry so blur and every
    // existing composite caller are untouched. Blend disabled: the
    // shader writes premultiplied `co`/`αo`, backdrop contribution
    // included (see `composite_blend.wgsl`).
    let blend_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-wgpu-blend-composite"),
        source: wgpu::ShaderSource::Wgsl(BLEND_COMPOSITE_WGSL.into()),
    });
    let blend_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-blend-layout"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 2,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let blend_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-wgpu-blend-pipe-layout"),
        bind_group_layouts: &[&ortho_layout, &blend_layout],
        push_constant_ranges: &[],
    });
    let blend_composite_pipeline =
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("riode-wgpu-blend-composite-pipeline"),
            layout: Some(&blend_pipe_layout),
            vertex: wgpu::VertexState {
                module: &blend_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[RectVertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blend_shader,
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

    // Custom (material-DSL) stage layout: bindings fixed by the engine
    // entry scaffold (`ENGINE_ENTRY_WGSL`) — 0 sampler (effect_sampler),
    // 1 uniforms, 2 input/card, 3 backdrop, 4 sharp. One layout for every
    // custom stage; the pipeline itself is per-source (renderer cache).
    let custom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-wgpu-custom-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 0,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 1,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 2,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 3,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 4,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let custom_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("riode-wgpu-custom-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
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
        filter_layout,
        blur_pipeline,
        composite_pipeline,
        blend_layout,
        blend_composite_pipeline,
        custom_layout,
        custom_sampler,
    }
}
