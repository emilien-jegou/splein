// Single responsibility: Dynamic uniform streaming and pipeline caching for materials and effects.

use rustc_hash::FxHashMap;

pub struct MaterialPipelineEntry {
    pub pipeline: wgpu::RenderPipeline,
    pub geo_bind_layout: wgpu::BindGroupLayout,
    pub uniform_bind_layout: Option<wgpu::BindGroupLayout>,
}

pub struct EffectPipelineEntry {
    pub pipeline: wgpu::RenderPipeline,
    pub geo_bind_layout: wgpu::BindGroupLayout,
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
        wgsl: &str,
        has_uniforms: bool,
        target_format: wgpu::TextureFormat,
    ) -> &MaterialPipelineEntry {
        self.pipelines.entry(wgsl.to_string()).or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("riode-material-module"),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });

            // Group 0: Dynamic ElementGeo uniform (256-byte aligned offset per draw)
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

            // Group 1: Dynamic Stage uniforms (256-byte aligned offset per draw)
            let uniform_bind_layout = if has_uniforms {
                Some(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("riode-material-uniform-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        binding: 0,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                }))
            } else {
                None
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
                uniform_bind_layout,
            }
        })
    }

    pub fn get_or_create_effect(
        &mut self,
        device: &wgpu::Device,
        wgsl: &str,
        has_uniforms: bool,
        target_format: wgpu::TextureFormat,
    ) -> &EffectPipelineEntry {
        self.effects.entry(wgsl.to_string()).or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("riode-effect-module"),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });

            // Group 0: Ortho + ElementGeo
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

            // Group 1: src_tex, dst_tex, sampler, and optional dynamic uniforms
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
                        min_binding_size: None,
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
                tex_bind_layout,
            }
        })
    }
}
