// Single responsibility: Software rasterization execution of Scene chunks via tiny-skia.

use tiny_skia::{Pixmap, Transform as SkiaTransform};

use crate::clip::ClipStack;
use crate::command::{execute_commands, CommandContext};
use crate::image::ImageCache;
use crate::shadow::ShadowRasterizer;
use crate::svg::SvgCache;
use crate::text::LineCompositeCache;
use aurora::foundation::DamageRegion;
use aurora::render::error::BackendError;
use aurora::scene::command::hash_commands;
use aurora::scene::scene::Scene;
use aurora::text::TextContext;

/// Software rasterizer driving scene chunks into CPU pixel buffers.
pub struct TinySkiaRenderer {
    persistent_canvas: Pixmap,
    clip_stack: ClipStack,
    pub text_ctx: TextContext,
    svg_cache: SvgCache,
    image_cache: ImageCache,
    shadow_rasterizer: ShadowRasterizer,
    line_cache: LineCompositeCache,
    tx_stack: Vec<SkiaTransform>,
    opacity_stack: Vec<f32>,
    scratch_pixmap: Option<Pixmap>,
    /// Per-chunk raster cache [STAGE1 #20]: chunk index → (content hash,
    /// chunk-local pixmap). Bounded: only chunks ≤512px per side are cached;
    /// larger chunks execute directly. Entries for `fresh` chunks are
    /// refreshed; stale indices are pruned each frame.
    chunk_cache: std::collections::HashMap<usize, (u64, Pixmap)>,
}

impl TinySkiaRenderer {
    /// Constructs a clean software rasterizer instance.
    pub fn new() -> Self {
        let mut canvas = Pixmap::new(1, 1).unwrap();
        canvas.fill(tiny_skia::Color::TRANSPARENT);
        Self {
            persistent_canvas: canvas,
            clip_stack: ClipStack::new(),
            text_ctx: TextContext::new(),
            svg_cache: SvgCache::new(),
            image_cache: ImageCache::new(),
            shadow_rasterizer: ShadowRasterizer::new(),
            line_cache: LineCompositeCache::new(),
            tx_stack: Vec::with_capacity(32),
            opacity_stack: Vec::with_capacity(32),
            scratch_pixmap: None,
            chunk_cache: std::collections::HashMap::new(),
        }
    }

    /// Reallocates persistent canvas while preserving shared bounds.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (nw, nh) = (width.max(1), height.max(1));
        if self.persistent_canvas.width() == nw && self.persistent_canvas.height() == nh {
            return;
        }
        let (ow, oh) = (
            self.persistent_canvas.width(),
            self.persistent_canvas.height(),
        );
        let mut new_c = Pixmap::new(nw, nh).expect("Failed to allocate Pixmap");
        new_c.fill(tiny_skia::Color::TRANSPARENT);
        let (cw, ch) = (ow.min(nw) as usize, oh.min(nh) as usize);
        let (src, dst) = (self.persistent_canvas.data(), new_c.data_mut());
        for y in 0..ch {
            let (si, di) = (y * ow as usize * 4, y * nw as usize * 4);
            dst[di..di + cw * 4].copy_from_slice(&src[si..si + cw * 4]);
        }
        self.persistent_canvas = new_c;
        self.chunk_cache.clear();
    }

    /// Accesses the backing CPU frame buffer.
    pub fn canvas(&self) -> &Pixmap {
        &self.persistent_canvas
    }

    /// Mutably accesses the backing CPU frame buffer.
    pub fn canvas_mut(&mut self) -> &mut Pixmap {
        &mut self.persistent_canvas
    }

    /// Updates the typography shaper and glyph database handle.
    pub fn set_text_context(&mut self, text_ctx: TextContext) {
        self.text_ctx = text_ctx;
    }

    /// Rasterizes damaged rectangles of the scene into the persistent canvas.
    #[tracing::instrument(name = "render_damage", skip_all)]
    pub fn render_damage(
        &mut self,
        scene: &Scene,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        if damage.is_empty() {
            return Ok(());
        }
        let _span = tracing::info_span!("Raster::TinySkia").entered();
        let (cw, ch) = (
            self.persistent_canvas.width(),
            self.persistent_canvas.height(),
        );

        // Scroll shifts first: the persistent canvas is current through last
        // frame (every frame repaints its own damage), so moving the covered
        // bulk by the rounded delta is exact, and the damage tiles below only
        // repaint the exposed strips. Exposed rows whose source falls outside
        // the clip are left for the tile pass; rounding residue (<=0.5px) is
        // covered by the 1px seam pad the damage plan adds to each strip.
        //
        // Integral device deltas only: a fractional memmove leaves subpixel
        // AA seams at every moved edge (thousands of stray pixels, not
        // theory). Fractional shifts repaint their whole clip box instead —
        // correct at full cost. This backend works in physical pixels
        // throughout, so logical integrality is device integrality.
        let mut tiles = DamageRegion::new();
        for rect in damage.rects() {
            tiles.push(*rect);
        }
        for shift in damage.scroll_shifts() {
            if shift.delta.x.fract() == 0.0 && shift.delta.y.fract() == 0.0 {
                self.blit_within_canvas(
                    &shift.clip,
                    shift.delta.x.round() as i32,
                    shift.delta.y.round() as i32,
                );
            } else {
                tiles.push(shift.clip);
            }
        }

        for rect in tiles.rects() {
            let (x, y) = (
                (rect.x.floor() as i32).max(0) as u32,
                (rect.y.floor() as i32).max(0) as u32,
            );
            let (r, b) = (
                (rect.right().ceil() as u32).min(cw),
                (rect.bottom().ceil() as u32).min(ch),
            );
            if r <= x || b <= y {
                continue;
            }
            let (rw, rh) = (r - x, b - y);

            let mut scratch = match self.scratch_pixmap.take() {
                Some(mut p) if p.width() >= rw && p.height() >= rh => {
                    // Avoid pinning a once-huge tile forever: if the retained pixmap is
                    // more than 4x the needed area, shrink it back to the tile size.
                    let (pw, ph) = (p.width() as u64, p.height() as u64);
                    if pw * ph > (rw as u64) * (rh as u64) * 4 {
                        Pixmap::new(
                            rw.next_power_of_two().max(64),
                            rh.next_power_of_two().max(64),
                        )
                        .unwrap()
                    } else {
                        p.fill(tiny_skia::Color::TRANSPARENT);
                        p
                    }
                }
                _ => Pixmap::new(
                    rw.next_power_of_two().max(64),
                    rh.next_power_of_two().max(64),
                )
                .unwrap(),
            };

            // Each tile starts from a clean clip and transform stack. Anything a previous
            // tile left behind would otherwise bleed across, and the damage region caps
            // the tile count at a small number, so the reset is free.
            self.clip_stack.reset();
            self.tx_stack.clear();
            self.opacity_stack.clear();

            self.tx_stack
                .push(SkiaTransform::from_translate(-(x as f32), -(y as f32)));
            let mut ctx = CommandContext {
                clip_stack: &mut self.clip_stack,
                text_ctx: &self.text_ctx,
                svg_cache: &mut self.svg_cache,
                image_cache: &mut self.image_cache,
                shadow_rasterizer: &mut self.shadow_rasterizer,
                tx_stack: &mut self.tx_stack,
                opacity_stack: &mut self.opacity_stack,
                line_cache: &mut self.line_cache,
            };

            for (ci, chunk) in scene
                .chunks
                .iter()
                .enumerate()
                .filter(|(_, c)| c.bounds.overlaps(rect))
            {
                // Chunk-local streams: placement (origin + hoisted root
                // transform) and absolute inherited clips are applied here,
                // around the content, so moved chunks reuse their encoding.
                let base = *ctx.tx_stack.last().unwrap();
                // [STAGE1 #20] Fast path: unchanged small chunk under pure
                // translation → blit cached chunk-local raster instead of
                // re-executing commands.
                if !chunk.fresh
                    && chunk.placement.b == 0.0
                    && chunk.placement.c == 0.0
                    && chunk.placement.a == 1.0
                    && chunk.placement.d == 1.0
                {
                    let cw_px = chunk.bounds.width.ceil() as u32;
                    let ch_px = chunk.bounds.height.ceil() as u32;
                    if cw_px > 0 && ch_px > 0 && cw_px <= 512 && ch_px <= 512 {
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        std::hash::Hash::hash(&hash_commands(&chunk.commands), &mut h);
                        std::hash::Hash::hash(
                            &(
                                chunk.abs_origin.x.to_bits(),
                                chunk.abs_origin.y.to_bits(),
                                chunk.placement.tx.to_bits(),
                                chunk.placement.ty.to_bits(),
                                chunk.inherited_clips.len(),
                            ),
                            &mut h,
                        );
                        for clip in chunk.inherited_clips.iter() {
                            std::hash::Hash::hash(
                                &(
                                    clip.rect.x.to_bits(),
                                    clip.rect.y.to_bits(),
                                    clip.rect.width.to_bits(),
                                    clip.rect.height.to_bits(),
                                ),
                                &mut h,
                            );
                        }
                        let key = std::hash::Hasher::finish(&h);
                        if let Some((kh, pix)) = self.chunk_cache.get(&ci) {
                            if *kh == key {
                                // Composite cached chunk-local raster into the
                                // tile scratch at its screen offset.
                                let sx = (chunk.bounds.x - x as f32).round() as i32;
                                let sy = (chunk.bounds.y - y as f32).round() as i32;
                                let (pw, ph) =
                                    (pix.width().min(cw_px), pix.height().min(ch_px));
                                let sdata = pix.data();
                                let tw = scratch.width() as usize;
                                let tdata = scratch.data_mut();
                                for row in 0..ph as usize {
                                    let dy = sy + row as i32;
                                    if dy < 0 || dy >= rh as i32 {
                                        continue;
                                    }
                                    let si = row * pix.width() as usize * 4;
                                    let di = (dy as usize * tw + sx.max(0) as usize) * 4;
                                    let skip_l = (-sx).max(0) as usize;
                                    let w = ((pw as usize).saturating_sub(skip_l)).min(
                                        tw.saturating_sub(sx.max(0) as usize),
                                    );
                                    if w == 0 {
                                        continue;
                                    }
                                    // Alpha-over composite (cached raster is
                                    // premultiplied like the canvas).
                                    for px in 0..w {
                                        let s =
                                            &sdata[(si + (skip_l + px) * 4)..(si + (skip_l + px) * 4 + 4)];
                                        let d = &mut tdata[(di + px * 4)..(di + px * 4 + 4)];
                                        let sa = s[3] as u32;
                                        if sa == 255 {
                                            d.copy_from_slice(s);
                                        } else if sa != 0 {
                                            let inv = 255 - sa;
                                            for k in 0..3 {
                                                d[k] = ((s[k] as u32 * sa
                                                    + d[k] as u32 * inv)
                                                    / 255)
                                                    as u8;
                                            }
                                            d[3] = (sa + d[3] as u32 * inv / 255).min(255) as u8;
                                        }
                                    }
                                }
                                continue;
                            }
                        }
                        // Miss: render chunk-local raster into cache entry,
                        // reusing the shared stacks (tile base already reset
                        // per tile above).
                        let mut cpix =
                            Pixmap::new(cw_px.max(1), ch_px.max(1)).unwrap();
                        {
                            ctx.tx_stack.push(SkiaTransform::from_translate(
                                -(chunk.abs_origin.x + chunk.placement.tx),
                                -(chunk.abs_origin.y + chunk.placement.ty),
                            ));
                            for clip in chunk.inherited_clips.iter() {
                                let b = *ctx.tx_stack.last().unwrap();
                                ctx.clip_stack.push(
                                    &clip.rect,
                                    clip.radius,
                                    b,
                                    cw_px,
                                    ch_px,
                                    cw_px,
                                    ch_px,
                                );
                            }
                            execute_commands(
                                &chunk.commands,
                                &mut cpix.as_mut(),
                                &mut ctx,
                                cw_px as f32,
                                ch_px as f32,
                            );
                            for _ in chunk.inherited_clips.iter() {
                                ctx.clip_stack.pop();
                            }
                            ctx.tx_stack.pop();
                        }
                        self.chunk_cache.insert(ci, (key, cpix));
                        // Fall through to direct execution for *this* tile to
                        // keep a single placement code path.
                    }
                }
                // Direct placement path (cache miss, fresh chunk, or large /
                // transformed chunk): execute into the tile scratch.
                let base = *ctx.tx_stack.last().unwrap();
                for clip in chunk.inherited_clips.iter() {
                    ctx.clip_stack.push(
                        &clip.rect,
                        clip.radius,
                        base,
                        rw,
                        rh,
                        scratch.width(),
                        scratch.height(),
                    );
                }
                let placed = base
                    .pre_translate(chunk.abs_origin.x, chunk.abs_origin.y)
                    .pre_concat(SkiaTransform::from_row(
                        chunk.placement.a,
                        chunk.placement.b,
                        chunk.placement.c,
                        chunk.placement.d,
                        chunk.placement.tx,
                        chunk.placement.ty,
                    ));
                ctx.tx_stack.push(placed);
                execute_commands(
                    &chunk.commands,
                    &mut scratch.as_mut(),
                    &mut ctx,
                    rw as f32,
                    rh as f32,
                );
                ctx.tx_stack.pop();
                for _ in chunk.inherited_clips.iter() {
                    ctx.clip_stack.pop();
                }
            }
            self.tx_stack.pop();
            self.clip_stack.reset();

            let (src, dst) = (scratch.data(), self.persistent_canvas.data_mut());
            for row in 0..rh as usize {
                let si = row * (scratch.width() * 4) as usize;
                let di = ((y as usize + row) * cw as usize + x as usize) * 4;
                dst[di..di + rw as usize * 4].copy_from_slice(&src[si..si + rw as usize * 4]);
            }
            self.scratch_pixmap = Some(scratch);
        }
        // Prune entries for chunks that no longer exist; fresh-chunk misses
        // were refreshed above. Bounded by construction (≤512px/side each).
        let n = scene.chunks.len();
        self.chunk_cache.retain(|&k, _| k < n);
        Ok(())
    }
    /// Moves retained pixels inside `clip` by (`dx`, `dy`) device px.
    ///
    /// Memmove semantics per row (`copy_within`), so overlapping shifts are
    /// exact. Destination rows whose source row falls outside `clip` are
    /// skipped — that is the newly-exposed band, repainted by the tile pass.
    /// A zero shift is a no-op; out-of-canvas clips are clamped, never panic.
    fn blit_within_canvas(&mut self, clip: &aurora::foundation::ResolvedRect, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        let (cw, ch) = (
            self.persistent_canvas.width() as i32,
            self.persistent_canvas.height() as i32,
        );
        let x0 = (clip.x.floor() as i32).max(0);
        let y0 = (clip.y.floor() as i32).max(0);
        let x1 = (clip.right().ceil() as i32).min(cw);
        let y1 = (clip.bottom().ceil() as i32).min(ch);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let stride = cw as usize * 4;
        let data = self.persistent_canvas.data_mut();
        // Visit order matters across rows: a source row must be read before it
        // is overwritten as a destination. dy > 0 pulls from above → descend;
        // dy < 0 pulls from below → ascend; dy == 0 is intra-row (copy_within
        // is overlap-safe) so either order works.
        if dy > 0 {
            for y in (y0..y1).rev() {
                Self::blit_row(data, stride, cw, x0, x1, y0, y1, y, y - dy, dx);
            }
        } else {
            for y in y0..y1 {
                Self::blit_row(data, stride, cw, x0, x1, y0, y1, y, y - dy, dx);
            }
        }
    }

    /// Copies one blit row: destination row `y` pulls the clip-clamped segment
    /// of source row `src_y`, shifted by `dx`. Skips rows/segments whose
    /// source lies outside the clip span (the exposed band, repainted next).
    #[allow(clippy::too_many_arguments)]
    fn blit_row(
        data: &mut [u8],
        stride: usize,
        canvas_w: i32,
        x0: i32,
        x1: i32,
        y0: i32,
        y1: i32,
        y: i32,
        src_y: i32,
        dx: i32,
    ) {
        if src_y < y0 || src_y >= y1 {
            return;
        }
        let sx0 = (x0 - dx).max(x0).max(0);
        let sx1 = (x1 - dx).min(x1).min(canvas_w);
        if sx1 <= sx0 {
            return;
        }
        let src = src_y as usize * stride + sx0 as usize * 4;
        let dst = y as usize * stride + (sx0 + dx) as usize * 4;
        let len = (sx1 - sx0) as usize * 4;
        data.copy_within(src..src + len, dst);
    }
}

impl Default for TinySkiaRenderer {
    fn default() -> Self {
        Self::new()
    }
}
