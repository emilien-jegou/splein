// Single responsibility: Main render loop with batched rect + glyph scene pass + blit.

use aurora::foundation::{Color, DamageRegion, ResolvedRect};
use aurora::render::BackendError;
use aurora::scene::scene::Scene;
use aurora::text::TextContext;

use crate::cache::{chunk_base, chunk_key, ChunkCache};
use crate::command::{Batch, Op};
use crate::context::WgpuContext;
use crate::pipeline::{create_pipelines, GlyphVertex, Pipelines, RRectVertex, RectVertex};
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
    gpu: WgpuContext,
    pipes: Pipelines,
    batch: Batch,
    atlas: GlyphAtlas,
    text_ctx: TextContext,
    ortho_buf: wgpu::Buffer,
    ortho_bg: wgpu::BindGroup,
    glyph_bg_linear: wgpu::BindGroup,
    glyph_bg_nearest: wgpu::BindGroup,
    vert_buf: wgpu::Buffer,
    vert_cap: u64,
    glyph_buf: wgpu::Buffer,
    glyph_cap: u64,
    rrect_buf: wgpu::Buffer,
    rrect_cap: u64,
    scale: f32,
    chunks: ChunkCache,
    /// Force a Clear of the persistent target on the next frame. Set at
    /// construction and whenever resize rebuilds the target (fresh texture
    /// memory is undefined). Without this, a Load would preserve garbage.
    clear_next: bool,
    /// Cached readback staging buffer (width, height, buffer) for offscreen use.
    readback: Option<(u32, u32, wgpu::Buffer)>,
}

fn make_ortho(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    w: u32,
    h: u32,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    use wgpu::util::DeviceExt;
    let data = [w.max(1) as f32, h.max(1) as f32, 0.0, 0.0];
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("aurora-wgpu-ortho"),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("aurora-wgpu-ortho-bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bg)
}

fn make_vert_buf(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.max(256),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl WgpuRenderer {
    pub fn new(gpu: WgpuContext, text_ctx: TextContext) -> Self {
        let pipes = create_pipelines(
            &gpu.device,
            wgpu::TextureFormat::Rgba8Unorm,
            gpu.config.format,
        );
        let (ortho_buf, ortho_bg) = make_ortho(
            &gpu.device,
            &pipes.ortho_layout,
            gpu.config.width,
            gpu.config.height,
        );
        let atlas = GlyphAtlas::new(&gpu.device);
        let glyph_bg_linear = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("aurora-wgpu-glyph-bg-linear"),
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
            label: Some("aurora-wgpu-glyph-bg-nearest"),
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
        let vert_cap = 4096u64;
        let vert_buf = make_vert_buf(
            &gpu.device,
            "aurora-wgpu-verts",
            vert_cap * std::mem::size_of::<RectVertex>() as u64,
        );
        let glyph_cap = 4096u64;
        let glyph_buf = make_vert_buf(
            &gpu.device,
            "aurora-wgpu-glyphs",
            glyph_cap * std::mem::size_of::<GlyphVertex>() as u64,
        );
        let rrect_cap = 1024u64;
        let rrect_buf = make_vert_buf(
            &gpu.device,
            "aurora-wgpu-rrects",
            rrect_cap * std::mem::size_of::<RRectVertex>() as u64,
        );
        Self {
            gpu,
            pipes,
            batch: Batch::new(),
            atlas,
            text_ctx,
            ortho_buf,
            ortho_bg,
            glyph_bg_linear,
            glyph_bg_nearest,
            vert_buf,
            vert_cap,
            glyph_buf,
            glyph_cap,
            rrect_buf,
            rrect_cap,
            scale: 1.0,
            chunks: ChunkCache::default(),
            clear_next: true,
            readback: None,
        }
    }

    /// Sets the layout→device scale (window scale factor). Takes effect
    /// next frame via the batch root transform.
    pub fn set_scale(&mut self, dsf: f32) {
        let dsf = if dsf.is_finite() {
            dsf.clamp(0.5, 4.0)
        } else {
            1.0
        };
        if (dsf - self.scale).abs() > 1e-4 {
            tracing::info!(scale = dsf, "aurora-wgpu device scale changed");
        }
        self.scale = dsf;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (ow, oh) = (self.gpu.config.width, self.gpu.config.height);
        self.gpu.resize(width, height);
        if ow != self.gpu.config.width || oh != self.gpu.config.height {
            let (buf, bg) = make_ortho(
                &self.gpu.device,
                &self.pipes.ortho_layout,
                self.gpu.config.width,
                self.gpu.config.height,
            );
            self.ortho_buf = buf;
            self.ortho_bg = bg;
            // Fresh target memory is undefined: the next frame must Clear
            // rather than Load, or garbage persists in undrawn pixels.
            self.clear_next = true;
            // Cached verts are baked in device px of the old size: drop
            // them so every chunk re-emits at the new size. (Atlas
            // coverage is size-independent and survives.)
            self.chunks.clear();
        }
    }

    fn ensure_rect_cap(&mut self, need: u64) {
        if need <= self.vert_cap {
            return;
        }
        let mut cap = self.vert_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.vert_buf = make_vert_buf(
            &self.gpu.device,
            "aurora-wgpu-verts",
            cap * std::mem::size_of::<RectVertex>() as u64,
        );
        self.vert_cap = cap;
    }

    fn ensure_glyph_cap(&mut self, need: u64) {
        if need <= self.glyph_cap {
            return;
        }
        let mut cap = self.glyph_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.glyph_buf = make_vert_buf(
            &self.gpu.device,
            "aurora-wgpu-glyphs",
            cap * std::mem::size_of::<GlyphVertex>() as u64,
        );
        self.glyph_cap = cap;
    }

    fn ensure_rrect_cap(&mut self, need: u64) {
        if need <= self.rrect_cap {
            return;
        }
        let mut cap = self.rrect_cap.max(1);
        while cap < need {
            cap *= 2;
        }
        self.rrect_buf = make_vert_buf(
            &self.gpu.device,
            "aurora-wgpu-rrects",
            cap * std::mem::size_of::<RRectVertex>() as u64,
        );
        self.rrect_cap = cap;
    }

    /// Bounds culling viewport in layout px: chunk bounds and damage are
    /// layout-space, while the swapchain config is device px.
    fn viewport(&self) -> ResolvedRect {
        let s = self.scale.max(0.5);
        ResolvedRect::new(
            0.0,
            0.0,
            self.gpu.config.width as f32 / s,
            self.gpu.config.height as f32 / s,
        )
    }

    /// Whether the persistent scene target must be cleared this frame.
    ///
    /// The target retains the previous frame, so a partial damage must
    /// LOAD to preserve undamaged regions (the damaged area is repainted
    /// from base by damage-base quads first). Clearing is correct only
    /// when every visible chunk repaints: empty damage (the backend only
    /// reaches here with `needs_present`, i.e. a full redraw) or right
    /// after the target was (re)created, whose memory is undefined.
    /// Clearing on any other damage punches holes everywhere damage
    /// doesn't cover, which later Load frames then preserve indefinitely.
    fn must_clear(&mut self, damage: &DamageRegion) -> bool {
        if self.clear_next {
            self.clear_next = false;
            return true;
        }
        damage.is_empty()
    }

    /// Device-px scissor rect for an optional clip/bounds rect.
    /// Rounds (not truncates) so fractional rects snap symmetrically;
    /// returns None when empty or fully outside the surface.
    fn scissor_rect(
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

    fn apply_scissor(
        pass: &mut wgpu::RenderPass<'_>,
        scissor: &Option<ResolvedRect>,
        sw: u32,
        sh: u32,
    ) -> bool {
        match Self::scissor_rect(scissor, sw, sh) {
            Some((x, y, w, h)) => {
                pass.set_scissor_rect(x, y, w, h);
                true
            }
            None => false,
        }
    }

    /// Shades the scene into the persistent target without presenting.
    ///
    /// Same batching and scene pass as [`Self::render_frame`] minus the
    /// swapchain acquire, blit and present: the quad-combine backend reads
    /// the persistent texture back to CPU (see [`Self::readback_rgba8`]).
    pub fn render_scene_offscreen(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        let viewport = self.viewport();
        // Resolve the one-shot clear flag before the per-field borrows
        // below (must_clear takes &mut self).
        let clear = self.must_clear(damage);

        // Batch scene content: viewport cull + damage cull, then per-chunk
        // vertex cache replay (static chunks skip command re-emit and
        // glyph re-raster entirely) or fresh emission on miss.
        // Disjoint field borrows keep borrowck happy.
        {
            let (batch, atlas, text_ctx, gpu, chunks) = (
                &mut self.batch,
                &mut self.atlas,
                &self.text_ctx,
                &self.gpu,
                &mut self.chunks,
            );
            atlas.next_frame();
            chunks.next_frame();
            batch.set_scale(self.scale);
            batch.reset();
            // Tile semantics for partial damage: repaint the damaged area
            // from base color first (opaque base quads), so vacated pixels
            // (removed/moved nodes) don't ghost under the Load below.
            // Decided here so the quads precede all chunk content.
            // `clear` was resolved before the per-field borrows below.
            if !clear {
                batch.emit_damage_base(damage, self.scale, base_color);
            }
            let sq16 = (aurora::text::quantize_raster_scale(self.scale) * 16.0).round() as u16;
            let agen = atlas.generation();
            for chunk in scene.chunks.iter() {
                // Viewport cull only. There is deliberately NO damage
                // cull here: repainting a lower chunk without repainting
                // the chunks above it would bury their stale pixels
                // wherever they overlap on screen (draws are full
                // geometry, not damage-scissored). Like vello, every
                // visible chunk repaints every presented frame, so
                // painter's order is correct by construction; damage
                // still drives present-skip and Load/Clear decisions.
                if !chunk.bounds.overlaps(&viewport) {
                    continue;
                }
                let key = chunk_key(chunk, sq16);
                // Chunk boundary: runs must not merge across chunks, or the
                // per-chunk snapshot below would slice verts whose draw ops
                // belong to the previous chunk (invisible on cache replay).
                batch.begin_chunk();
                // Origin-rebased replay: a hit at a new origin translates
                // cached verts instead of re-tessellating (scroll); a hit at
                // the baked origin is a plain copy. Anything rebase_for
                // rejects (scale change, local frames, fractional glyph
                // phase) falls through to fresh emission below.
                let replayed = match chunks.get(key, agen) {
                    Some(hit) => match hit.rebase_for(chunk.abs_origin, batch.scale()) {
                        Some((0.0, 0.0)) => {
                            let rb = batch.rect_verts.len() as u32;
                            let gb = batch.glyph_verts.len() as u32;
                            let qb = batch.rrect_verts.len() as u32;
                            batch.rect_verts.extend_from_slice(&hit.rects);
                            batch.glyph_verts.extend_from_slice(&hit.glyphs);
                            batch.rrect_verts.extend_from_slice(&hit.rrects);
                            // Extend in place (N2): the temp Vec was a per-chunk
                            // alloc on the replay path.
                            batch.ops.extend(
                                hit.ops.iter().map(|op| ChunkCache::rebased(op, rb, gb, qb)),
                            );
                            true
                        }
                        Some((dx, dy)) => {
                            let rb = batch.rect_verts.len() as u32;
                            let gb = batch.glyph_verts.len() as u32;
                            let qb = batch.rrect_verts.len() as u32;
                            hit.append_rebased(
                                dx,
                                dy,
                                &mut batch.rect_verts,
                                &mut batch.glyph_verts,
                                &mut batch.rrect_verts,
                            );
                            // Extend in place (N2): the temp Vec was a per-chunk
                            // alloc on the replay path.
                            batch.ops.extend(
                                hit.ops.iter().map(|op| ChunkCache::rebased(op, rb, gb, qb)),
                            );
                            true
                        }
                        None => false,
                    },
                    None => false,
                };
                if replayed {
                    continue;
                }
                let (r0, g0, q0, o0) = (
                    batch.rect_verts.len(),
                    batch.glyph_verts.len(),
                    batch.rrect_verts.len(),
                    batch.ops.len(),
                );
                for clip in chunk.inherited_clips.iter() {
                    batch.push_clip_device(&clip.rect, clip.radius);
                }
                batch.push_base(chunk_base(chunk));
                batch.commands(&chunk.commands, text_ctx, atlas, &gpu.queue);
                batch.pop_base();
                for _ in chunk.inherited_clips.iter() {
                    batch.clip.pop();
                }
                // Snapshot the new ranges with buffer-relative op starts.
                let r0u = r0 as u32;
                let g0u = g0 as u32;
                let q0u = q0 as u32;
                let rects = batch.rect_verts[r0..].to_vec();
                let glyphs = batch.glyph_verts[g0..].to_vec();
                let rrects = batch.rrect_verts[q0..].to_vec();
                let ops: Vec<Op> = batch.ops[o0..]
                    .iter()
                    .map(|op| match *op {
                        // BaseQuads are emitted before the chunk loop, so a
                        // chunk snapshot can never contain one; pass through
                        // defensively (never replayed from cache).
                        Op::BaseQuads { start, count } => Op::BaseQuads {
                            start: start - r0u,
                            count,
                        },
                        Op::RRects {
                            start,
                            count,
                            scissor,
                        } => Op::RRects {
                            start: start - q0u,
                            count,
                            scissor,
                        },
                        Op::Rects {
                            start,
                            count,
                            scissor,
                        } => Op::Rects {
                            start: start - r0u,
                            count,
                            scissor,
                        },
                        Op::Glyphs {
                            start,
                            count,
                            scissor,
                            linear,
                        } => Op::Glyphs {
                            start: start - g0u,
                            count,
                            scissor,
                            linear,
                        },
                    })
                    .collect();
                chunks.insert(
                    key,
                    crate::cache::CachedChunk::new(
                        rects,
                        glyphs,
                        rrects,
                        ops,
                        atlas.generation(),
                        chunk.abs_origin,
                        batch.scale(),
                        crate::cache::rebaseable_stream(&chunk.commands),
                    ),
                );
            }
        }

        // Upload verts + ortho before opening the pass.
        if !self.batch.rect_verts.is_empty() {
            self.ensure_rect_cap(self.batch.rect_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.vert_buf,
                0,
                bytemuck::cast_slice(&self.batch.rect_verts),
            );
        }
        if !self.batch.glyph_verts.is_empty() {
            self.ensure_glyph_cap(self.batch.glyph_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.glyph_buf,
                0,
                bytemuck::cast_slice(&self.batch.glyph_verts),
            );
        }
        if !self.batch.rrect_verts.is_empty() {
            self.ensure_rrect_cap(self.batch.rrect_verts.len() as u64);
            self.gpu.queue.write_buffer(
                &self.rrect_buf,
                0,
                bytemuck::cast_slice(&self.batch.rrect_verts),
            );
        }
        {
            let wh = [
                self.gpu.config.width as f32,
                self.gpu.config.height as f32,
                0.0,
                0.0,
            ];
            self.gpu
                .queue
                .write_buffer(&self.ortho_buf, 0, bytemuck::cast_slice(&wh));
        }

        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("aurora-wgpu-frame"),
            });

        // Scene pass into persistent target, ops in command order.
        // Partial damage Loads (preserves undamaged regions, with the
        // damaged area pre-painted to base above); empty damage or a fresh
        // target Clears (every visible chunk repaints). `clear` was
        // resolved once above so the one-shot flag can't double-fire.
        let scene_load = if clear {
            wgpu::LoadOp::Clear(to_wgpu(base_color))
        } else {
            wgpu::LoadOp::Load
        };
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("aurora-wgpu-scene"),
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
            });
            let (sw, sh) = (self.gpu.config.width, self.gpu.config.height);
            for op in self.batch.ops.iter() {
                match op {
                    Op::BaseQuads { start, count } => {
                        pass.set_scissor_rect(0, 0, sw, sh);
                        pass.set_pipeline(&self.pipes.rect_replace_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.vert_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::Rects {
                        start,
                        count,
                        scissor,
                    } => {
                        if !Self::apply_scissor(&mut pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.rect_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.vert_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::RRects {
                        start,
                        count,
                        scissor,
                    } => {
                        if !Self::apply_scissor(&mut pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.rrect_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_vertex_buffer(0, self.rrect_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Op::Glyphs {
                        start,
                        count,
                        scissor,
                        linear,
                    } => {
                        if !Self::apply_scissor(&mut pass, scissor, sw, sh) {
                            continue;
                        }
                        pass.set_pipeline(&self.pipes.glyph_pipeline);
                        pass.set_bind_group(0, &self.ortho_bg, &[]);
                        pass.set_bind_group(
                            1,
                            if *linear {
                                &self.glyph_bg_linear
                            } else {
                                &self.glyph_bg_nearest
                            },
                            &[],
                        );
                        pass.set_vertex_buffer(0, self.glyph_buf.slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                }
            }
        }

        self.gpu.queue.submit(Some(enc.finish()));
        Ok(())
    }

    /// Persistent target size in pixels (quadrant size in quad-combine use).
    pub fn target_size(&self) -> (u32, u32) {
        (self.gpu.config.width.max(1), self.gpu.config.height.max(1))
    }

    /// Copies the persistent texture to CPU as tightly-packed RGBA bytes.
    ///
    /// Must follow [`Self::render_scene_offscreen`] (which submits the scene
    /// pass; this only adds the copy + a blocking map). Staging buffer is
    /// cached across frames and rebuilt on size change. The quad-combine
    /// backend composites these bytes with the other engines into one window.
    pub fn readback_rgba8(&mut self) -> Result<Vec<u8>, BackendError> {
        let (w, h) = self.target_size();
        // COPY_BYTES_PER_ROW_ALIGNMENT: rows are padded to 256B.
        let pitch = ((w * 4 + 255) / 256 * 256) as usize;
        let size = (pitch * h as usize) as u64;
        let buf = match &self.readback {
            Some((bw, bh, b)) if *bw == w && *bh == h => b,
            _ => {
                let b = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("aurora-wgpu-readback"),
                    size: size.max(256),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                self.readback = Some((w, h, b));
                &self.readback.as_ref().unwrap().2
            }
        };
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("aurora-wgpu-readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.gpu.persistent,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch as u32),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit(Some(encoder.finish()));

        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.gpu.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| BackendError::raster("readback map channel closed"))?
            .map_err(|e| BackendError::raster(format!("readback map failed: {e:?}")))?;
        let out = {
            let mapped = slice.get_mapped_range();
            let mut tight = vec![0u8; (w * h * 4) as usize];
            for y in 0..h as usize {
                tight[y * w as usize * 4..(y + 1) * w as usize * 4]
                    .copy_from_slice(&mapped[y * pitch..y * pitch + w as usize * 4]);
            }
            tight
        };
        buf.unmap();
        Ok(out)
    }

    pub fn render_frame(
        &mut self,
        scene: &Scene,
        base_color: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        let surface = self.gpu.surface.as_ref().expect(
            "render_frame needs a window surface; headless contexts use render_scene_offscreen",
        );
        let frame = match surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                surface.configure(&self.gpu.device, &self.gpu.config);
                surface.get_current_texture().map_err(|e| {
                    BackendError::surface(format!("swapchain recovery failed: {e:?}"))
                })?
            }
            Err(e) => {
                return Err(BackendError::surface(format!(
                    "swapchain acquire failed: {e:?}"
                )))
            }
        };
        self.render_scene_offscreen(scene, base_color, damage)?;

        // Blit persistent target -> swapchain.
        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("aurora-wgpu-blit"),
            });
        {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bg = self
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("aurora-wgpu-blit-bg"),
                    layout: &self.pipes.blit_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.gpu.persistent_view),
                    }],
                });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("aurora-wgpu-blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
}
