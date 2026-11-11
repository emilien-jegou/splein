// Responsibility: Executes command batch rendering, scissor clipping, bounded backdrops, and layer composites.

use riode::foundation::ResolvedRect;
use tracing::instrument;

use crate::command::{Batch, Op};
use crate::context::WgpuContext;
use crate::material_pipeline::MaterialPipelineCache;
use crate::pipeline::Pipelines;
use crate::renderer::buffers::StagingBuffers;
use crate::renderer::layers::{LayerPool, OpenLayer};
use crate::renderer::material_runner::MaterialRunner;

pub fn scissor_rect(scissor: &Option<ResolvedRect>, sw: u32, sh: u32) -> Option<(u32, u32, u32, u32)> {
    match scissor {
        Some(r) => {
            let x = r.x.max(0.0).round() as u32;
            let y = r.y.max(0.0).round() as u32;
            let xe = (r.x + r.width).max(0.0).round() as u32;
            let ye = (r.y + r.height).max(0.0).round() as u32;
            let w = xe.saturating_sub(x).min(sw.saturating_sub(x.min(sw)));
            let h = ye.saturating_sub(y).min(sh.saturating_sub(y.min(sh)));
            if w == 0 || h == 0 {
                return None;
            }
            Some((x, y, w, h))
        }
        None => Some((0, 0, sw, sh)),
    }
}

pub fn apply_scissor(pass: &mut wgpu::RenderPass<'_>, scissor: &Option<ResolvedRect>, sw: u32, sh: u32) -> bool {
    match scissor_rect(scissor, sw, sh) {
        Some((x, y, w, h)) => {
            pass.set_scissor_rect(x, y, w, h);
            true
        }
        None => false,
    }
}

#[inline(always)]
unsafe fn erase_pass<'p, 'q>(p: wgpu::RenderPass<'p>) -> wgpu::RenderPass<'q> {
    std::mem::transmute(p)
}

pub struct ScenePassExecutor<'a> {
    pub gpu: &'a WgpuContext,
    pub pipes: &'a Pipelines,
    pub batch: &'a Batch,
    pub buffers: &'a StagingBuffers,
    pub layers: &'a mut LayerPool,
    pub materials: &'a mut MaterialPipelineCache,
    pub glyph_bg_linear: &'a wgpu::BindGroup,
    pub glyph_bg_nearest: &'a wgpu::BindGroup,
    pub scale: f32,
    pub sw: u32,
    pub sh: u32,
}

impl<'a> ScenePassExecutor<'a> {
    #[instrument(skip_all, fields(ops = %self.batch.ops.len(), sw = %self.sw, sh = %self.sh))]
    pub fn execute(mut self, enc: &mut wgpu::CommandEncoder, scene_load: wgpu::LoadOp<wgpu::Color>) {
        let mut root_layer_regions: Vec<ResolvedRect> = Vec::new();
        let mut kept: Vec<wgpu::BindGroup> = Vec::new();
        let runner = MaterialRunner {
            gpu: self.gpu,
            buffers: self.buffers,
            scale: self.scale,
            sw: self.sw,
            sh: self.sh,
        };

        let enc_ptr: *mut wgpu::CommandEncoder = enc;
        let init_pass = unsafe {
            (&mut *enc_ptr).begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("riode-wgpu-scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.gpu.persistent_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: scene_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            })
        };

        let mut pass = Some(unsafe { erase_pass(init_pass) });

        for op in self.batch.ops.iter() {
            match op {
                Op::BeginLayer { region, paint } => {
                    drop(pass.take());
                    self.execute_begin_layer(enc_ptr, &mut root_layer_regions, region, paint);
                    let slot = self.layers.stack.last().unwrap().slot;
                    let layer_view = &self.layers.pool[slot].view;
                    let mut p = unsafe {
                        (&mut *enc_ptr).begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("riode-wgpu-layer"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: layer_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                        })
                    };

                    if (paint.shape.is_some() || paint.material.is_some())
                        && apply_scissor(&mut p, &Some(*region), self.sw, self.sh)
                    {
                        runner.draw_material(self.materials, &mut p, region, paint);
                    }
                    pass = Some(unsafe { erase_pass(p) });
                }
                Op::EndLayer { start, count, scissor, region, paint } => {
                    if paint.overlay.as_ref().is_some_and(|ov| !ov.is_empty()) {
                        let p = pass.as_mut().unwrap();
                        if apply_scissor(p, &Some(*region), self.sw, self.sh) {
                            runner.draw_overlay(self.materials, p, region, paint);
                        }
                    }
                    drop(pass.take());
                    pass = self.execute_end_layer(
                        enc_ptr,
                        &runner,
                        &mut kept,
                        scissor,
                        region,
                        paint,
                        *start,
                        *count,
                    );
                }
                Op::BaseQuads { start, count } => {
                    let p = pass.as_mut().unwrap();
                    p.set_scissor_rect(0, 0, self.sw, self.sh);
                    p.set_pipeline(&self.pipes.rect_replace_pipeline);
                    p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                    p.set_vertex_buffer(0, self.buffers.vert_buf.slice(..));
                    p.draw(*start..*start + *count, 0..1);
                }
                Op::Glyphs { start, count, scissor, linear } => {
                    let p = pass.as_mut().unwrap();
                    if apply_scissor(p, scissor, self.sw, self.sh) {
                        p.set_pipeline(&self.pipes.glyph_pipeline);
                        p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                        p.set_bind_group(1, if *linear { self.glyph_bg_linear } else { self.glyph_bg_nearest }, &[]);
                        p.set_vertex_buffer(0, self.buffers.glyph_buf.slice(..));
                        p.draw(*start..*start + *count, 0..1);
                    }
                }
                Op::Images { start, count, scissor, bind } => {
                    let p = pass.as_mut().unwrap();
                    if apply_scissor(p, scissor, self.sw, self.sh) {
                        if let Some(img) = self.batch.image_binds.get(*bind as usize) {
                            p.set_pipeline(&self.pipes.image_pipeline);
                            p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                            p.set_bind_group(1, &img.bind, &[]);
                            p.set_vertex_buffer(0, self.buffers.image_buf.slice(..));
                            p.draw(*start..*start + *count, 0..1);
                        }
                    }
                }
            }
        }
        self.layers.stale_root_layers = root_layer_regions;
    }

    fn execute_begin_layer(
        &mut self,
        enc_ptr: *mut wgpu::CommandEncoder,
        root_layer_regions: &mut Vec<ResolvedRect>,
        region: &ResolvedRect,
        paint: &riode::foundation::LayerPaint,
    ) {
        let needs_bd = paint.effect.as_ref().is_some_and(|e| e.requires_backdrop())
            || paint.material.as_ref().is_some_and(|m| m.stages().iter().any(|s| s.requires_backdrop()));

        let slot = self.layers.stack.len();
        self.layers.ensure_slot(&self.gpu.device, &self.pipes.filter_layout, slot + 1, self.sw, self.sh);

        if needs_bd {
            let parent_tex = match self.layers.stack.last() {
                Some(l) => &self.layers.pool[l.slot].texture,
                None => &self.gpu.persistent,
            };
            let bd_slot = slot + 1;
            let margin = 32.0f32;
            let rx = (region.x - margin).max(0.0).floor() as u32;
            let ry = (region.y - margin).max(0.0).floor() as u32;
            let rw = (((region.x + region.width + margin).ceil() as u32).saturating_sub(rx))
                .min(self.sw.saturating_sub(rx)).max(1);
            let rh = (((region.y + region.height + margin).ceil() as u32).saturating_sub(ry))
                .min(self.sh.saturating_sub(ry)).max(1);

            unsafe {
                (&mut *enc_ptr).copy_texture_to_texture(
                    wgpu::ImageCopyTexture {
                        texture: parent_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: rx, y: ry, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::ImageCopyTexture {
                        texture: &self.layers.pool[bd_slot].texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: rx, y: ry, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d { width: rw, height: rh, depth_or_array_layers: 1 },
                );
            }
        }

        if self.layers.stack.is_empty() {
            root_layer_regions.push(*region);
        }
        self.layers.stack.push(OpenLayer { slot });
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_end_layer(
        &mut self,
        enc_ptr: *mut wgpu::CommandEncoder,
        runner: &MaterialRunner<'_>,
        kept: &mut Vec<wgpu::BindGroup>,
        scissor: &Option<ResolvedRect>,
        region: &ResolvedRect,
        paint: &riode::foundation::LayerPaint,
        start: u32,
        count: u32,
    ) -> Option<wgpu::RenderPass<'static>> {
        let open = self.layers.stack.pop()?;
        let parent_view = self.layers.parent_view(&self.gpu.persistent_view);

        let mut p = unsafe {
            (&mut *enc_ptr).begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("riode-wgpu-composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: parent_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            })
        };

        if apply_scissor(&mut p, scissor, self.sw, self.sh) {
            let layer_slot = &self.layers.pool[open.slot];
            if paint.effect.as_ref().is_some_and(|e| !e.is_empty()) {
                let bd_slot = open.slot + 1;
                let bd_view = if bd_slot < self.layers.pool.len() {
                    &self.layers.pool[bd_slot].view
                } else {
                    &layer_slot.view
                };
                runner.draw_effect()
                    .materials(&mut *self.materials)
                    .pass(&mut p)
                    .kept(kept)
                    .region(region)
                    .src_view(&layer_slot.view)
                    .dst_view(bd_view)
                    .paint(paint)
                    .start(start)
                    .count(count)
                    .call();
            } else {
                p.set_pipeline(&self.pipes.composite_pipeline);
                p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                p.set_vertex_buffer(0, self.buffers.vert_buf.slice(..));
                p.set_bind_group(1, &layer_slot.composite_bg, &[]);
                p.draw(start..start + count, 0..1);
            }
        }
        Some(unsafe { erase_pass(p) })
    }
}
