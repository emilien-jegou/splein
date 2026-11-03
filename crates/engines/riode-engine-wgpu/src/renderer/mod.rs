// Single responsibility: Main render loop orchestrating scene batches, offscreens and presentations.

pub mod buffers;
pub mod layers;
pub mod material_runner;
pub mod readback;
pub mod scene_pass;

use riode::foundation::{Color, DamageRegion, ResolvedRect};
use riode::render::BackendError;
use riode::scene::scene::Scene;
use riode::text::TextContext;

use crate::cache::{chunk_key, ChunkCache};
use crate::command::Batch;
use crate::context::WgpuContext;
use crate::image::ImageCache;
use crate::material_pipeline::MaterialPipelineCache;
use crate::pipeline::{create_pipelines, Pipelines};
use crate::renderer::buffers::StagingBuffers;
use crate::renderer::layers::LayerPool;
use crate::renderer::readback::ReadbackBuffer;
use crate::renderer::scene_pass::ScenePassExecutor;
use crate::svg::SvgCache;
use crate::text::GlyphAtlas;

fn to_wgpu(c: Color) -> wgpu::Color {
    wgpu::Color {
        r: c.r as f64,
        g: c.g as f64,
        b: c.b as f64,
        a: c.a as f64,
    }
}

pub struct WgpuRenderer {
    pub gpu: WgpuContext,
    pub pipes: Pipelines,
    pub batch: Batch,
    pub atlas: GlyphAtlas,
    pub text_ctx: TextContext,
    pub buffers: StagingBuffers,
    pub glyph_bg_linear: wgpu::BindGroup,
    pub glyph_bg_nearest: wgpu::BindGroup,
    pub images: ImageCache,
    pub svgs: SvgCache,
    pub scale: f32,
    pub chunks: ChunkCache,
    pub clear_next: bool,
    pub readback: ReadbackBuffer,
    pub layers: LayerPool,
    pub materials: MaterialPipelineCache,
}

impl WgpuRenderer {
    pub fn new(gpu: WgpuContext, text_ctx: TextContext) -> Self {
        let pipes = create_pipelines(
            &gpu.device,
            wgpu::TextureFormat::Rgba8Unorm,
            gpu.config.format,
        );
        let buffers = StagingBuffers::new(
            &gpu.device,
            &pipes.ortho_layout,
            gpu.config.width,
            gpu.config.height,
        );
        let atlas = GlyphAtlas::new(&gpu.device, &text_ctx);

        let glyph_bg_linear = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-wgpu-glyph-bg-linear"),
            layout: &pipes.glyph_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipes.glyph_sampler_linear),
                },
            ],
        });
        let glyph_bg_nearest = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-wgpu-glyph-bg-nearest"),
            layout: &pipes.glyph_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipes.glyph_sampler_nearest),
                },
            ],
        });

        Self {
            buffers,
            pipes,
            batch: Batch::new(),
            atlas,
            text_ctx,
            glyph_bg_linear,
            glyph_bg_nearest,
            images: ImageCache::new(&gpu.device),
            svgs: SvgCache::new(),
            scale: 1.0,
            chunks: ChunkCache::default(),
            clear_next: true,
            readback: ReadbackBuffer::new(),
            layers: LayerPool::new(),
            materials: MaterialPipelineCache::default(),
            gpu,
        }
    }

    pub fn set_scale(&mut self, dsf: f32) {
        let dsf = if dsf.is_finite() {
            dsf.clamp(0.5, 4.0)
        } else {
            1.0
        };
        self.scale = dsf;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (ow, oh) = (self.gpu.config.width, self.gpu.config.height);
        self.gpu.resize(width, height);
        if ow != self.gpu.config.width || oh != self.gpu.config.height {
            let (buf, bg) = StagingBuffers::create_ortho(
                &self.gpu.device,
                &self.pipes.ortho_layout,
                self.gpu.config.width,
                self.gpu.config.height,
            );
            self.buffers.ortho_buf = buf;
            self.buffers.ortho_bg = bg;
            self.clear_next = true;
            self.chunks.clear();
            self.layers.clear();
        }
    }

    pub fn render_scene_offscreen(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        let clear = if self.clear_next {
            self.clear_next = false;
            true
        } else {
            damage.is_empty()
        };
        let stale = std::mem::take(&mut self.layers.stale_root_layers);

        // Reset dynamic uniform cursors at the start of the frame.
        self.buffers.reset_material_cursors();

        self.atlas.next_frame();
        self.chunks.next_frame();
        self.images.next_frame();
        self.svgs.next_frame();
        self.batch.set_scale(self.scale);
        self.batch.reset();

        if !clear {
            self.batch.emit_damage_base(damage, self.scale, base_color);
            for r in &stale {
                self.batch.emit_base_rect_dev(r, base_color);
            }
        }

        let viewport = ResolvedRect::new(
            0.0,
            0.0,
            self.gpu.config.width as f32 / self.scale,
            self.gpu.config.height as f32 / self.scale,
        );

        for chunk in scene.chunks.iter() {
            if !chunk.bounds.overlaps(&viewport) {
                continue;
            }
            self.batch.begin_chunk();
            for clip in chunk.inherited_clips.iter() {
                self.batch.push_clip_device(&clip.rect);
            }
            self.batch.emit_chunk(
                &riode::scene::chunk_base(chunk),
                &chunk.commands,
                &self.text_ctx,
                &mut self.atlas,
                &self.gpu.queue,
                &self.gpu.device,
                &mut self.images,
                &mut self.svgs,
            );
            for _ in chunk.inherited_clips.iter() {
                self.batch.clip.pop();
            }
        }

        // Upload vertex buffers.
        if !self.batch.rect_verts.is_empty() {
            self.buffers
                .ensure_rect_cap(&self.gpu.device, self.batch.rect_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.buffers.vert_buf,
                0,
                bytemuck::cast_slice(&self.batch.rect_verts),
            );
        }
        if !self.batch.glyph_verts.is_empty() {
            self.buffers
                .ensure_glyph_cap(&self.gpu.device, self.batch.glyph_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.buffers.glyph_buf,
                0,
                bytemuck::cast_slice(&self.batch.glyph_verts),
            );
        }
        if !self.batch.image_verts.is_empty() {
            self.buffers
                .ensure_image_cap(&self.gpu.device, self.batch.image_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.buffers.image_buf,
                0,
                bytemuck::cast_slice(&self.batch.image_verts),
            );
        }
        let wh = [
            self.gpu.config.width as f32,
            self.gpu.config.height as f32,
            0.0,
            0.0,
        ];
        self.gpu
            .queue
            .write_buffer(&self.buffers.ortho_buf, 0, bytemuck::cast_slice(&wh));

        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("riode-frame"),
            });
        let load_op = if clear {
            wgpu::LoadOp::Clear(to_wgpu(base_color))
        } else {
            wgpu::LoadOp::Load
        };

        ScenePassExecutor {
            gpu: &self.gpu,
            pipes: &self.pipes,
            batch: &self.batch,
            buffers: &self.buffers,
            layers: &mut self.layers,
            materials: &mut self.materials,
            glyph_bg_linear: &self.glyph_bg_linear,
            glyph_bg_nearest: &self.glyph_bg_nearest,
            scale: self.scale,
            sw: self.gpu.config.width.max(1),
            sh: self.gpu.config.height.max(1),
        }
        .execute(&mut enc, load_op);

        self.gpu.queue.submit(Some(enc.finish()));
        Ok(())
    }

    pub fn render_frame(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        let surface = self
            .gpu
            .surface
            .as_ref()
            .expect("render_frame requires a presentation surface");
        let frame = match surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                surface.configure(&self.gpu.device, &self.gpu.config);
                surface
                    .get_current_texture()
                    .map_err(|e| BackendError::surface(format!("recovery failed: {e:?}")))?
            }
            Err(e) => return Err(BackendError::surface(format!("acquire failed: {e:?}"))),
        };

        self.render_scene_offscreen(scene, base_color, damage)?;

        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit"),
            });
        {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bg = self
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("blit-bg"),
                    layout: &self.pipes.blit_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.gpu.persistent_view),
                    }],
                });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipes.blit_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..6, 0..1);
        }
        self.gpu.queue.submit(Some(enc.finish()));
        frame.present();
        Ok(())
    }

    pub fn readback_rgba8(&mut self) -> Result<Vec<u8>, BackendError> {
        self.readback.readback_rgba8(&self.gpu)
    }
}
