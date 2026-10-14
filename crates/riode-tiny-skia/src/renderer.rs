// Single responsibility: Software rasterization execution of Scene chunks via tiny-skia.

use tiny_skia::{Pixmap, Transform as SkiaTransform};

use crate::clip::ClipStack;
use crate::command::{execute_commands, CommandContext};
use crate::image::ImageCache;
use crate::shadow::ShadowRasterizer;
use crate::svg::SvgCache;
use crate::text::LineCompositeCache;
use riode::foundation::DamageRegion;
use riode::render::error::BackendError;
use riode::scene::scene::Scene;
use riode::text::TextContext;
use riode::scene::SceneChunk;
use riode::foundation::ResolvedRect;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

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
        }
    }

    /// Reallocates persistent canvas while preserving shared bounds.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (nw, nh) = (width.max(1), height.max(1));
        // Keep retained text-line compositing bounded by one viewport-sized
        // RGBA plane rather than the fixed 8 MiB legacy budget.
        self.line_cache.set_cap(((nw as usize) * (nh as usize) * 4).clamp(256 * 1024, 8 << 20));
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

    #[cfg(feature = "parallel")]
    fn render_damage_parallel(
        &mut self,
        scene: &Scene,
        tiles: &[ResolvedRect],
        cw: u32,
        ch: u32,
    ) -> Result<(), BackendError> {
        let text_ctx = self.text_ctx.clone();
        // Snapshot the chunks a tile needs as owned values so Rayon can
        // borrow them by value instead of sharing `Scene` (whose command
        // stream uses Rc and is not Sync).
        let tile_chunks: Vec<(ResolvedRect, Vec<SceneChunk>)> = tiles
            .iter()
            .map(|rect| {
                let chunks = scene
                    .chunks
                    .iter()
                    .filter(|chunk| chunk.bounds.overlaps(rect))
                    .cloned()
                    .collect();
                (*rect, chunks)
            })
            .collect();
        let workers = rayon::current_num_threads().clamp(1, tiles.len().max(1));
        let results = tile_chunks
            .par_iter()
            .map_init(
                || TileWorker {
                    scratch: None,
                    clip_stack: ClipStack::new(),
                    tx_stack: Vec::with_capacity(32),
                    opacity_stack: Vec::with_capacity(32),
                    text_ctx: text_ctx.clone(),
                    svg_cache: SvgCache::new(),
                    image_cache: ImageCache::new(),
                    shadow_rasterizer: ShadowRasterizer::new(),
                    line_cache: LineCompositeCache::new(),
                },
                |worker, tile| -> Result<(u32, u32, u32, u32, tiny_skia::Pixmap), BackendError> {
                    let rect = &tile.0;
                    let (x, y) = (
                        (rect.x.floor() as i32).max(0) as u32,
                        (rect.y.floor() as i32).max(0) as u32,
                    );
                    let (r, b) = (
                        (rect.right().ceil() as u32).min(cw),
                        (rect.bottom().ceil() as u32).min(ch),
                    );
                    if r <= x || b <= y {
                        return Ok((x, y, 0, 0, tiny_skia::Pixmap::new(1, 1).unwrap()));
                    }
                    let (rw, rh) = (r - x, b - y);
                    let qx = tile_quantum(rw).max(1);
                    let qy = tile_quantum(rh).max(1);
                    let mut scratch = worker
                        .scratch
                        .take()
                        .unwrap_or_else(|| tiny_skia::Pixmap::new(qx, qy).unwrap());
                    if scratch.width() < qx || scratch.height() < qy {
                        scratch = tiny_skia::Pixmap::new(qx, qy).unwrap();
                    }
                    scratch.fill(tiny_skia::Color::TRANSPARENT);
                    worker.clip_stack.reset();
                    worker.tx_stack.clear();
                    worker.opacity_stack.clear();
                    worker.tx_stack.push(tiny_skia::Transform::identity());
                    worker.tx_stack.push(tiny_skia::Transform::from_translate(
                        -(x as f32),
                        -(y as f32),
                    ));
                    worker.tx_stack.pop();
                    // Recompute transform stack in the same order as serial code.
                    worker.tx_stack.clear();
                    worker.opacity_stack.clear();
                    worker.tx_stack.push(tiny_skia::Transform::from_translate(
                        -(x as f32),
                        -(y as f32),
                    ));
                    worker.opacity_stack.push(1.0);
                    let mut ctx = CommandContext {
                        clip_stack: &mut worker.clip_stack,
                        text_ctx: &worker.text_ctx,
                        svg_cache: &mut worker.svg_cache,
                        image_cache: &mut worker.image_cache,
                        shadow_rasterizer: &mut worker.shadow_rasterizer,
                        tx_stack: &mut worker.tx_stack,
                        opacity_stack: &mut worker.opacity_stack,
                        line_cache: &mut worker.line_cache,
                    };
                    for chunk in &tile.1 {
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
                            .pre_concat(tiny_skia::Transform::from_row(
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
                    worker.tx_stack.pop();
                    worker.clip_stack.reset();
                    Ok((x, y, rw, rh, scratch))
                },
            )
            .collect::<Result<Vec<_>, BackendError>>()?;

        let dst = self.persistent_canvas.data_mut();
        let cw = cw as usize;
        for (x, y, rw, rh, scratch) in results {
            if rw == 0 || rh == 0 {
                continue;
            }
            let src = scratch.data();
            let src_stride = scratch.width() as usize * 4;
            let di_base = (y as usize) * cw * 4 + (x as usize) * 4;
            for row in 0..rh as usize {
                let si = row * src_stride;
                let di = di_base + row * (cw * 4);
                dst[di..di + (rw as usize) * 4]
                    .copy_from_slice(&src[si..si + (rw as usize) * 4]);
            }
        }
        Ok(())
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

#[cfg(feature = "parallel")]
        if tiles.rects().len() > 1 {
            let tiles = tiles.rects().to_vec();
            return self.render_damage_parallel(scene, &tiles, cw, ch);
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
                        Pixmap::new(tile_quantum(rw), tile_quantum(rh)).unwrap()
                    } else {
                        p.fill(tiny_skia::Color::TRANSPARENT);
                        p
                    }
                }
                _ => Pixmap::new(tile_quantum(rw), tile_quantum(rh)).unwrap(),
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

            for chunk in scene.chunks.iter().filter(|c| c.bounds.overlaps(rect)) {
                // Chunk-local streams: placement (origin + hoisted root
                // transform) and absolute inherited clips are applied here,
                // around the content, so moved chunks reuse their encoding.
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
        Ok(())
    }
    /// Moves retained pixels inside `clip` by (`dx`, `dy`) device px.
    ///
    /// Memmove semantics per row (`copy_within`), so overlapping shifts are
    /// exact. Destination rows whose source row falls outside `clip` are
    /// skipped — that is the newly-exposed band, repainted by the tile pass.
    /// A zero shift is a no-op; out-of-canvas clips are clamped, never panic.
    fn blit_within_canvas(&mut self, clip: &riode::foundation::ResolvedRect, dx: i32, dy: i32) {
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

/// Tile scratch quantum (M2): sizes cluster on 64px steps so retained
/// pixmaps reuse across frames, while waste stays under one 64px strip per
/// edge (~2 MB worst case at 4K) instead of next-pow2 padding (a fullscreen
/// tile pinned 4096×4096×4 = 64 MiB for a 31.6 MiB canvas).
#[inline(always)]
fn tile_quantum(n: u32) -> u32 {
    n.next_multiple_of(64).max(64)
}


#[cfg(feature = "parallel")]
struct TileWorker {
    scratch: Option<tiny_skia::Pixmap>,
    clip_stack: ClipStack,
    tx_stack: Vec<tiny_skia::Transform>,
    opacity_stack: Vec<f32>,
    text_ctx: TextContext,
    svg_cache: SvgCache,
    image_cache: ImageCache,
    shadow_rasterizer: ShadowRasterizer,
    line_cache: LineCompositeCache,
}
