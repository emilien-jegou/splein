// Single responsibility: Software rasterization execution of Scene chunks via tiny-skia.

use tiny_skia::{Pixmap, Transform as SkiaTransform};

use aurora::foundation::DamageRegion;
use aurora::render::error::BackendError;
use crate::clip::ClipStack;
use crate::command::{execute_commands, CommandContext};
use crate::image::ImageCache;
use crate::shadow::ShadowRasterizer;
use crate::svg::SvgCache;
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
            tx_stack: Vec::with_capacity(32),
            opacity_stack: Vec::with_capacity(32),
            scratch_pixmap: None,
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

        for rect in damage.rects() {
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
}

impl Default for TinySkiaRenderer {
    fn default() -> Self {
        Self::new()
    }
}
