// Responsibility: Dispatches material, overlay, and post-effect passes using pre-cached persistent bind groups.

use bon::bon;
use riode::foundation::{LayerPaint, ResolvedRect};

use crate::context::WgpuContext;
use crate::material_pipeline::MaterialPipelineCache;
use crate::renderer::buffers::StagingBuffers;

pub struct MaterialRunner<'a> {
    pub gpu: &'a WgpuContext,
    pub buffers: &'a StagingBuffers,
    pub scale: f32,
    pub sw: u32,
    pub sh: u32,
}

#[bon]
impl<'a> MaterialRunner<'a> {
    pub fn draw_material(
        &self,
        materials: &mut MaterialPipelineCache,
        pass: &mut wgpu::RenderPass<'_>,
        region: &ResolvedRect,
        paint: &LayerPaint,
    ) {
        let has_shape = paint.shape.as_ref().is_some_and(|s| !s.is_empty());
        let has_material = paint.material.as_ref().is_some_and(|m| !m.is_empty());
        if !has_shape && !has_material {
            return;
        }

        let composed = riode_shader::compose_material_shader(paint);
        let entry = materials.get_or_create(
            &self.gpu.device,
            &self.buffers.material_geo_buf,
            &self.buffers.material_uniform_buf,
            &composed.wgsl,
            composed.total_uniform_size > 0,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        let geo_offset = self.buffers.alloc_geo_offset();
        let geo_data = [
            region.x,
            region.y,
            region.width,
            region.height,
            self.sw as f32,
            self.sh as f32,
            self.scale,
            0.0f32,
        ];
        self.gpu.queue.write_buffer(
            &self.buffers.material_geo_buf,
            geo_offset as u64,
            bytemuck::cast_slice(&geo_data),
        );

        let u_offset = if entry.uniform_bind_group.is_some() {
            let u_size = composed.total_uniform_size.max(16);
            let u_offset = self.buffers.alloc_uniform_offset(u_size);
            let mut uniform_bytes = vec![0u8; u_size];

            let geo = riode::foundation::Geo::new(region.width, region.height, [0.0; 4], self.scale);
            riode_shader::write_material_uniforms(paint, &geo, &mut uniform_bytes);

            self.gpu.queue.write_buffer(
                &self.buffers.material_uniform_buf,
                u_offset as u64,
                &uniform_bytes,
            );
            Some(u_offset)
        } else {
            None
        };

        unsafe {
            let pass_dyn: &mut wgpu::RenderPass<'_> = std::mem::transmute(pass);
            let pipe_dyn: &wgpu::RenderPipeline = std::mem::transmute(&entry.pipeline);
            let geo_dyn: &wgpu::BindGroup = std::mem::transmute(&entry.geo_bind_group);

            pass_dyn.set_pipeline(pipe_dyn);
            pass_dyn.set_bind_group(0, geo_dyn, &[geo_offset]);

            if let Some(u_offset) = u_offset {
                if let Some(ref u_bg) = entry.uniform_bind_group {
                    let u_dyn: &wgpu::BindGroup = std::mem::transmute(u_bg);
                    pass_dyn.set_bind_group(1, u_dyn, &[u_offset]);
                }
            }

            pass_dyn.draw(0..6, 0..1);
        }
    }

    pub fn draw_overlay(
        &self,
        materials: &mut MaterialPipelineCache,
        pass: &mut wgpu::RenderPass<'_>,
        region: &ResolvedRect,
        paint: &LayerPaint,
    ) {
        let Some(overlay) = paint.overlay.as_ref() else {
            return;
        };
        if overlay.is_empty() {
            return;
        }
        let composed = riode_shader::compose_overlay_shader(paint);
        let entry = materials.get_or_create(
            &self.gpu.device,
            &self.buffers.material_geo_buf,
            &self.buffers.material_uniform_buf,
            &composed.wgsl,
            composed.total_uniform_size > 0,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        let geo_offset = self.buffers.alloc_geo_offset();
        let geo_data = [
            region.x,
            region.y,
            region.width,
            region.height,
            self.sw as f32,
            self.sh as f32,
            self.scale,
            0.0f32,
        ];
        self.gpu.queue.write_buffer(
            &self.buffers.material_geo_buf,
            geo_offset as u64,
            bytemuck::cast_slice(&geo_data),
        );

        let u_offset = if entry.uniform_bind_group.is_some() {
            let u_size = composed.total_uniform_size.max(16);
            let u_offset = self.buffers.alloc_uniform_offset(u_size);
            let mut uniform_bytes = vec![0u8; u_size];

            let geo = riode::foundation::Geo::new(region.width, region.height, [0.0; 4], self.scale);
            riode_shader::write_overlay_uniforms(paint, &geo, &mut uniform_bytes);

            self.gpu.queue.write_buffer(
                &self.buffers.material_uniform_buf,
                u_offset as u64,
                &uniform_bytes,
            );
            Some(u_offset)
        } else {
            None
        };

        unsafe {
            let pass_dyn: &mut wgpu::RenderPass<'_> = std::mem::transmute(pass);
            let pipe_dyn: &wgpu::RenderPipeline = std::mem::transmute(&entry.pipeline);
            let geo_dyn: &wgpu::BindGroup = std::mem::transmute(&entry.geo_bind_group);

            pass_dyn.set_pipeline(pipe_dyn);
            pass_dyn.set_bind_group(0, geo_dyn, &[geo_offset]);

            if let Some(u_offset) = u_offset {
                if let Some(ref u_bg) = entry.uniform_bind_group {
                    let u_dyn: &wgpu::BindGroup = std::mem::transmute(u_bg);
                    pass_dyn.set_bind_group(1, u_dyn, &[u_offset]);
                }
            }

            pass_dyn.draw(0..6, 0..1);
        }
    }

    #[builder]
    pub fn draw_effect(
        &self,
        materials: &mut MaterialPipelineCache,
        pass: &mut wgpu::RenderPass<'_>,
        kept: &mut Vec<wgpu::BindGroup>,
        region: &ResolvedRect,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        paint: &LayerPaint,
        start: u32,
        count: u32,
    ) {
        let composed = riode_shader::compose_effect_shader(paint);
        let entry = materials.get_or_create_effect(
            &self.gpu.device,
            &self.buffers.ortho_buf,
            &self.buffers.material_geo_buf,
            &composed.wgsl,
            composed.total_uniform_size > 0,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );

        let geo_offset = self.buffers.alloc_geo_offset();
        let geo_data = [
            region.x,
            region.y,
            region.width,
            region.height,
            self.sw as f32,
            self.sh as f32,
            self.scale,
            0.0f32,
        ];
        self.gpu.queue.write_buffer(
            &self.buffers.material_geo_buf,
            geo_offset as u64,
            bytemuck::cast_slice(&geo_data),
        );

        let u_size = composed.total_uniform_size.max(16);
        let u_offset = self.buffers.alloc_uniform_offset(u_size);
        let mut uniform_bytes = vec![0u8; u_size];
        let geo = riode::foundation::Geo::new(region.width, region.height, [0.0; 4], self.scale);
        riode_shader::write_effect_uniforms(paint, &geo, &mut uniform_bytes);

        self.gpu.queue.write_buffer(
            &self.buffers.material_uniform_buf,
            u_offset as u64,
            &uniform_bytes,
        );

        let sampler = self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("riode-effect-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let mut g1_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(dst_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ];

        if composed.total_uniform_size > 0 {
            g1_entries.push(wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.buffers.material_uniform_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(256),
                }),
            });
        }

        let g1_bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-effect-g1-bg"),
            layout: &entry.tex_bind_layout,
            entries: &g1_entries,
        });

        unsafe {
            let pass_dyn: &mut wgpu::RenderPass<'_> = std::mem::transmute(pass);
            let pipe_dyn: &wgpu::RenderPipeline = std::mem::transmute(&entry.pipeline);
            let g0_dyn: &wgpu::BindGroup = std::mem::transmute(&entry.geo_bind_group);
            let g1_dyn: &wgpu::BindGroup = std::mem::transmute(&g1_bg);

            pass_dyn.set_pipeline(pipe_dyn);
            pass_dyn.set_vertex_buffer(0, self.buffers.vert_buf.slice(..));
            pass_dyn.set_bind_group(0, g0_dyn, &[geo_offset]);
            if composed.total_uniform_size > 0 {
                pass_dyn.set_bind_group(1, g1_dyn, &[u_offset]);
            } else {
                pass_dyn.set_bind_group(1, g1_dyn, &[]);
            }

            pass_dyn.draw(start..start + count, 0..1);
        }

        // Keep the bind group alive until the end of the frame!
        kept.push(g1_bg);
    }
}
