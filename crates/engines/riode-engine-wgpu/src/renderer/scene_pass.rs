// Single responsibility: Main scene op loop, layer management, overlays, and effect composites.

use riode::foundation::ResolvedRect;

use crate::command::{Batch, Op};
use crate::context::WgpuContext;
use crate::material_pipeline::MaterialPipelineCache;
use crate::pipeline::Pipelines;
use crate::renderer::buffers::StagingBuffers;
use crate::renderer::layers::{LayerPool, OpenLayer};
use crate::renderer::material_runner::MaterialRunner;

pub fn scissor_rect(
    scissor: &Option<ResolvedRect>,
    sw: u32,
    sh: u32,
) -> Option<(u32, u32, u32, u32)> {
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

pub fn apply_scissor(
    pass: &mut wgpu::RenderPass<'_>,
    scissor: &Option<ResolvedRect>,
    sw: u32,
    sh: u32,
) -> bool {
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
    pub fn execute(
        self,
        enc: &mut wgpu::CommandEncoder,
        scene_load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let mut root_layer_regions: Vec<ResolvedRect> = Vec::new();

        {
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

                        // 1. Snapshot parent view if effect or stage requires backdrop
                        let needs_bd = paint.effect.as_ref().is_some_and(|e| e.requires_backdrop())
                            || paint
                                .material
                                .as_ref()
                                .is_some_and(|m| m.stages().iter().any(|s| s.requires_backdrop()));

                        let slot = self.layers.stack.len();
                        self.layers
                            .ensure_slot(&self.gpu.device, slot + 1, self.sw, self.sh);

                        if needs_bd {
                            let parent_tex = match self.layers.stack.last() {
                                Some(l) => &self.layers.pool[l.slot].0,
                                None => &self.gpu.persistent,
                            };
                            let bd_slot = slot + 1; // Dedicated backdrop snapshot texture
                            unsafe {
                                (&mut *enc_ptr).copy_texture_to_texture(
                                    wgpu::ImageCopyTexture {
                                        texture: parent_tex,
                                        mip_level: 0,
                                        origin: wgpu::Origin3d::ZERO,
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    wgpu::ImageCopyTexture {
                                        texture: &self.layers.pool[bd_slot].0,
                                        mip_level: 0,
                                        origin: wgpu::Origin3d::ZERO,
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    wgpu::Extent3d {
                                        width: self.sw,
                                        height: self.sh,
                                        depth_or_array_layers: 1,
                                    },
                                );
                            }
                        }

                        if self.layers.stack.is_empty() {
                            root_layer_regions.push(*region);
                        }
                        self.layers.stack.push(OpenLayer { slot });

                        // 2. Open offscreen layer pass and clear
                        let layer_view = &self.layers.pool[slot].1;
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

                        // 3. Draw container surface (shape + material) BEFORE children
                        if paint.shape.is_some() || paint.material.is_some() {
                            if apply_scissor(&mut p, &Some(*region), self.sw, self.sh) {
                                runner.draw_material(
                                    self.materials,
                                    &mut p,
                                    &mut kept,
                                    region,
                                    paint,
                                );
                            }
                        }

                        pass = Some(unsafe { erase_pass(p) });
                    }
                    Op::EndLayer {
                        start,
                        count,
                        scissor,
                        region,
                        paint,
                    } => {
                        // 1. Overlay runs inside the offscreen layer AFTER children, clipped to shape
                        if paint.overlay.as_ref().is_some_and(|ov| !ov.is_empty()) {
                            let p = pass.as_mut().unwrap();
                            if apply_scissor(p, &Some(*region), self.sw, self.sh) {
                                runner.draw_overlay(
                                    self.materials,
                                    p,
                                    &mut kept,
                                    region,
                                    paint,
                                );
                            }
                        }

                        // Close offscreen layer pass
                        drop(pass.take());
                        let Some(open) = self.layers.stack.pop() else {
                            continue;
                        };
                        let parent_view = self.layers.parent_view(&self.gpu.persistent_view);

                        // 2. Open parent render pass
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

                        // 3. Composite offscreen onto parent: plain or through Effect shader
                        // TEMP DIAGNOSTIC (clip-escape hunt; remove after fix):
                        // same escape check for the layer composite quad — this
                        // is what paints row boxes; escaping it is the reported
                        // bottom overflow.
                        static NONE_C: std::sync::atomic::AtomicU32 =
                            std::sync::atomic::AtomicU32::new(0);
                        if scissor.is_none()
                            && NONE_C.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 40
                        {
                            tracing::warn!(
                                "TEMP composite with NO scissor: region=({:.0},{:.0})+{:.0}x{:.0}",
                                region.x,
                                region.y,
                                region.width,
                                region.height
                            );
                        }
                        if let Some(sc) = scissor {
                            let (mut x0, mut y0, mut x1, mut y1) =
                                (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                            for v in &self.batch.rect_verts[*start as usize
                                ..(*start + *count).min(self.batch.rect_verts.len() as u32)
                                    as usize]
                            {
                                x0 = x0.min(v.pos[0]);
                                y0 = y0.min(v.pos[1]);
                                x1 = x1.max(v.pos[0]);
                                y1 = y1.max(v.pos[1]);
                            }
                            if x0 < sc.x - 1.0
                                || y0 < sc.y - 1.0
                                || x1 > sc.x + sc.width + 1.0
                                || y1 > sc.y + sc.height + 1.0
                            {
                                tracing::warn!(
                                    "TEMP composite escape: quad=({x0:.0},{y0:.0})-({x1:.0},{y1:.0}) scissor=({:.0},{:.0})+{:.0}x{:.0} region=({:.0},{:.0})+{:.0}x{:.0}",
                                    sc.x,
                                    sc.y,
                                    sc.width,
                                    sc.height,
                                    region.x,
                                    region.y,
                                    region.width,
                                    region.height
                                );
                            }
                        }
                        if apply_scissor(&mut p, scissor, self.sw, self.sh) {
                            let layer_view = &self.layers.pool[open.slot].1;
                            let has_effect =
                                paint.effect.as_ref().is_some_and(|e| !e.is_empty());

                            if has_effect {
                                let bd_slot = open.slot + 1;
                                let bd_view = if bd_slot < self.layers.pool.len() {
                                    &self.layers.pool[bd_slot].1
                                } else {
                                    layer_view
                                };
                                runner.draw_effect(
                                    self.materials,
                                    &mut p,
                                    &mut kept,
                                    region,
                                    layer_view,
                                    bd_view,
                                    paint,
                                    *start,
                                    *count,
                                );
                            } else {
                                let bg = self.gpu.device.create_bind_group(
                                    &wgpu::BindGroupDescriptor {
                                        label: Some("riode-wgpu-layer-composite"),
                                        layout: &self.pipes.filter_layout,
                                        entries: &[wgpu::BindGroupEntry {
                                            binding: 0,
                                            resource: wgpu::BindingResource::TextureView(layer_view),
                                        }],
                                    },
                                );
                                let bg_dyn: &wgpu::BindGroup = unsafe { std::mem::transmute(&bg) };
                                p.set_pipeline(&self.pipes.composite_pipeline);
                                p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                                p.set_vertex_buffer(0, self.buffers.vert_buf.slice(..));
                                p.set_bind_group(1, bg_dyn, &[]);
                                p.draw(*start..*start + *count, 0..1);
                                kept.push(bg);
                            }
                        }
                        pass = Some(unsafe { erase_pass(p) });
                    }
                    Op::BaseQuads { start, count } => {
                        let p = pass.as_mut().unwrap();
                        p.set_scissor_rect(0, 0, self.sw, self.sh);
                        p.set_pipeline(&self.pipes.rect_replace_pipeline);
                        p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                        p.set_vertex_buffer(0, self.buffers.vert_buf.slice(..));
                        p.draw(*start..*start + *count, 0..1);
                    }
                    Op::Glyphs {
                        start,
                        count,
                        scissor,
                        linear,
                    } => {
                        let p = pass.as_mut().unwrap();
                        // TEMP DIAGNOSTIC (clip-escape hunt; remove after fix):
                        // a glyph quad outside its op scissor paints invisible
                        // (cut) — the fingerprint of lost text; inside a layer
                        // offscreen it also explains boxes-without-text.
                        if let Some(sc) = scissor {
                            let (mut x0, mut y0, mut x1, mut y1) =
                                (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                            for v in &self.batch.glyph_verts[*start as usize
                                ..(*start + *count).min(self.batch.glyph_verts.len() as u32)
                                    as usize]
                            {
                                x0 = x0.min(v.pos[0]);
                                y0 = y0.min(v.pos[1]);
                                x1 = x1.max(v.pos[0]);
                                y1 = y1.max(v.pos[1]);
                            }
                            if x0 < sc.x - 1.0
                                || y0 < sc.y - 1.0
                                || x1 > sc.x + sc.width + 1.0
                                || y1 > sc.y + sc.height + 1.0
                            {
                                tracing::warn!(
                                    "TEMP glyph escape: quad=({x0:.0},{y0:.0})-({x1:.0},{y1:.0}) scissor=({:.0},{:.0})+{:.0}x{:.0}",
                                    sc.x,
                                    sc.y,
                                    sc.width,
                                    sc.height
                                );
                            }
                        } else {
                            static NONE_W: std::sync::atomic::AtomicU32 =
                                std::sync::atomic::AtomicU32::new(0);
                            if NONE_W.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 40 {
                                tracing::warn!(
                                    "TEMP glyph op with NO scissor: start={start} count={count}"
                                );
                            }
                        }
                        if !apply_scissor(p, scissor, self.sw, self.sh) {
                            continue;
                        }
                        p.set_pipeline(&self.pipes.glyph_pipeline);
                        p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                        p.set_bind_group(
                            1,
                            if *linear {
                                self.glyph_bg_linear
                            } else {
                                self.glyph_bg_nearest
                            },
                            &[],
                        );
                        p.set_vertex_buffer(0, self.buffers.glyph_buf.slice(..));
                        p.draw(*start..*start + *count, 0..1);
                    }
                    Op::Images {
                        start,
                        count,
                        scissor,
                        bind,
                    } => {
                        let p = pass.as_mut().unwrap();
                        if !apply_scissor(p, scissor, self.sw, self.sh) {
                            continue;
                        }
                        let Some(img) = self.batch.image_binds.get(*bind as usize) else {
                            continue;
                        };
                        p.set_pipeline(&self.pipes.image_pipeline);
                        p.set_bind_group(0, &self.buffers.ortho_bg, &[]);
                        p.set_bind_group(1, &img.bind, &[]);
                        p.set_vertex_buffer(0, self.buffers.image_buf.slice(..));
                        p.draw(*start..*start + *count, 0..1);
                    }
                }
            }
        } // `pass` and `kept` are dropped here

        self.layers.stale_root_layers = root_layer_regions;
    }
}
