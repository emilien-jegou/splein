// Responsibility: Caches compiled WGSL shaders, pipelines, and permanent dynamic uniform bind groups.

use rustc_hash::FxHashMap;

pub struct MaterialPipelineEntry {
    pub pipeline: wgpu::RenderPipeline,
    pub geo_bind_layout: wgpu::BindGroupLayout,
    pub geo_bind_group: wgpu::BindGroup,
    pub uniform_bind_layout: Option<wgpu::BindGroupLayout>,
    pub uniform_bind_group: Option<wgpu::BindGroup>,
}

pub struct EffectPipelineEntry {
    pub pipeline: wgpu::RenderPipeline,
    pub geo_bind_layout: wgpu::BindGroupLayout,
    pub geo_bind_group: wgpu::BindGroup,
    pub tex_bind_layout: wgpu::BindGroupLayout,
}

#[derive(Default)]
pub struct MaterialPipelineCache {
    pipelines: FxHashMap<String, MaterialPipelineEntry>,
    effects: FxHashMap<String, EffectPipelineEntry>,
}

impl MaterialPipelineCache {
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        material_geo_buf: &wgpu::Buffer,
        material_uniform_buf: &wgpu::Buffer,
        wgsl: &str,
        has_uniforms: bool,
        target_format: wgpu::TextureFormat,
    ) -> &MaterialPipelineEntry {
        self.pipelines.entry(wgsl.to_string()).or_insert_with(|| {
            create_material_pipeline(
                device,
                material_geo_buf,
                material_uniform_buf,
                wgsl,
                has_uniforms,
                target_format,
            )
        })
    }

    pub fn get_or_create_effect(
        &mut self,
        device: &wgpu::Device,
        ortho_buf: &wgpu::Buffer,
        material_geo_buf: &wgpu::Buffer,
        wgsl: &str,
        has_uniforms: bool,
        target_format: wgpu::TextureFormat,
    ) -> &EffectPipelineEntry {
        self.effects.entry(wgsl.to_string()).or_insert_with(|| {
            create_effect_pipeline(device, ortho_buf, material_geo_buf, wgsl, has_uniforms, target_format)
        })
    }
}

fn create_material_pipeline(
    device: &wgpu::Device,
    material_geo_buf: &wgpu::Buffer,
    material_uniform_buf: &wgpu::Buffer,
    wgsl: &str,
    has_uniforms: bool,
    target_format: wgpu::TextureFormat,
) -> MaterialPipelineEntry {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-material-module"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });

    let geo_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-material-geo-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            binding: 0,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(32),
            },
            count: None,
        }],
    });

    let geo_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("riode-material-geo-bg-cached"),
        layout: &geo_bind_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: material_geo_buf,
                offset: 0,
                size: wgpu::BufferSize::new(32),
            }),
        }],
    });

    let (uniform_bind_layout, uniform_bind_group) = if has_uniforms {
        let u_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("riode-material-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 0,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(256),
                },
                count: None,
            }],
        });
        let u_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-material-uniform-bg-cached"),
            layout: &u_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: material_uniform_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(256),
                }),
            }],
        });
        (Some(u_layout), Some(u_bg))
    } else {
        (None, None)
    };

    let mut layouts = vec![&geo_bind_layout];
    if let Some(ref u_layout) = uniform_bind_layout {
        layouts.push(u_layout);
    }

    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-material-pipe-layout"),
        bind_group_layouts: &layouts,
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-material-pipeline"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
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

    MaterialPipelineEntry {
        pipeline,
        geo_bind_layout,
        geo_bind_group,
        uniform_bind_layout,
        uniform_bind_group,
    }
}

fn create_effect_pipeline(
    device: &wgpu::Device,
    ortho_buf: &wgpu::Buffer,
    material_geo_buf: &wgpu::Buffer,
    wgsl: &str,
    has_uniforms: bool,
    target_format: wgpu::TextureFormat,
) -> EffectPipelineEntry {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riode-effect-module"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });

    let geo_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-effect-geo-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::VERTEX,
                binding: 0,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::FRAGMENT,
                binding: 1,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(32),
                },
                count: None,
            },
        ],
    });

    let geo_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("riode-effect-g0-bg-cached"),
        layout: &geo_bind_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: ortho_buf,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: material_geo_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(32),
                }),
            },
        ],
    });

    let mut g1_entries = vec![
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
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ];

    if has_uniforms {
        g1_entries.push(wgpu::BindGroupLayoutEntry {
            visibility: wgpu::ShaderStages::FRAGMENT,
            binding: 3,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(256),
            },
            count: None,
        });
    }

    let tex_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("riode-effect-tex-layout"),
        entries: &g1_entries,
    });

    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riode-effect-pipe-layout"),
        bind_group_layouts: &[&geo_bind_layout, &tex_bind_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riode-effect-pipeline"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            compilation_options: Default::default(),
            buffers: &[crate::pipeline::RectVertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
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

    EffectPipelineEntry {
        pipeline,
        geo_bind_layout,
        geo_bind_group,
        tex_bind_layout,
    }
}
