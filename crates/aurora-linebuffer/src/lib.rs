// Single responsibility: Scanline rasterization of Scene chunks for
// memory-constrained displays (SPI LCDs, e-paper, small framebuffers).
//
// Unlike retained tile backends, this backend holds no full-frame canvas:
// each damaged scanline is rasterized into one `width × 1` row buffer and
// pushed to a [`LineSink`] immediately. Peak raster memory is ~`width × 4`
// bytes plus one row scratch (vs `width × height × 4` for a full frame).
//
// Rows execute through the same `aurora-tiny-skia` command executor as the
// tile backend (same clips, transforms, text, shadows, SVG), so pixels are
// identical — only the tiling differs. Scroll shifts are repainted as plain
// damage rows (no canvas to blit from); that is the documented trade for
// holding no frame history.

pub mod sink;

use aurora::foundation::DamageRegion;
use aurora::render::error::BackendError;
use aurora::scene::scene::Scene;
use aurora::text::TextContext;
use aurora_tiny_skia::clip::ClipStack;
use aurora_tiny_skia::command::{execute_commands, CommandContext};
use aurora_tiny_skia::image::ImageCache;
use aurora_tiny_skia::shadow::ShadowRasterizer;
use aurora_tiny_skia::svg::SvgCache;
use aurora_tiny_skia::text::LineCompositeCache;
pub use sink::{FramebufferSink, LineSink};
use tiny_skia::{Pixmap, Transform as SkiaTransform};

/// Scanline rasterizer pushing rows directly to a [`LineSink`].
pub struct LineBufferBackend {
    width: u32,
    height: u32,
    clip_stack: ClipStack,
    pub text_ctx: TextContext,
    svg_cache: SvgCache,
    image_cache: ImageCache,
    shadow_rasterizer: ShadowRasterizer,
    line_cache: LineCompositeCache,
    tx_stack: Vec<SkiaTransform>,
    opacity_stack: Vec<f32>,
    row: Pixmap,
}

impl LineBufferBackend {
    /// Constructs a scanline rasterizer for a `width × height` display.
    pub fn new(width: u32, height: u32) -> Self {
        let (w, h) = (width.max(1), height.max(1));
        let mut row = Pixmap::new(w, 1).expect("row buffer allocation");
        row.fill(tiny_skia::Color::TRANSPARENT);
        Self {
            width: w,
            height: h,
            clip_stack: ClipStack::new(),
            text_ctx: TextContext::new(),
            svg_cache: SvgCache::new(),
            image_cache: ImageCache::new(),
            shadow_rasterizer: ShadowRasterizer::new(),
            line_cache: LineCompositeCache::new(),
            tx_stack: Vec::with_capacity(32),
            opacity_stack: Vec::with_capacity(32),
            row,
        }
    }

    /// Updates the typography shaper and glyph database handle.
    pub fn set_text_context(&mut self, text_ctx: TextContext) {
        self.text_ctx = text_ctx;
    }

    /// Display width in physical pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Display height in physical pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Rasterizes damaged rows of the scene, pushing each to `sink`.
    ///
    /// Damage rects expand to full row spans (a row is the unit of work);
    /// scroll shifts repaint their clip rows (no retained canvas to blit).
    /// Rows with no overlapping chunk still push (transparent) so
    /// write-only displays (no readback) converge without a clear pass.
    #[tracing::instrument(name = "render_rows", skip_all)]
    pub fn render_damage(
        &mut self,
        scene: &Scene,
        damage: &DamageRegion,
        sink: &mut dyn LineSink,
    ) -> Result<(), BackendError> {
        if damage.is_empty() {
            return Ok(());
        }
        let _span = tracing::info_span!("Raster::LineBuffer").entered();

        // Union damage rects + scroll-shift clips into a row set.
        let mut rows = Vec::<bool>::new();
        rows.resize(self.height as usize, false);
        let mut mark = |y0: i32, y1: i32| {
            for y in y0.max(0)..y1.min(self.height as i32) {
                rows[y as usize] = true;
            }
        };
        for rect in damage.rects() {
            mark(rect.y.floor() as i32, rect.bottom().ceil() as i32);
        }
        for shift in damage.scroll_shifts() {
            mark(
                shift.clip.y.floor() as i32,
                shift.clip.bottom().ceil() as i32,
            );
        }

        for (y, &dirty) in rows.iter().enumerate() {
            if !dirty {
                continue;
            }
            self.render_row(scene, y as u32, sink)?;
        }
        sink.flush();
        Ok(())
    }

    /// Rasterizes one scanline into the row buffer and pushes it.
    fn render_row(
        &mut self,
        scene: &Scene,
        y: u32,
        sink: &mut dyn LineSink,
    ) -> Result<(), BackendError> {
        let w = self.width;
        self.row.fill(tiny_skia::Color::TRANSPARENT);
        self.clip_stack.reset();
        self.tx_stack.clear();
        self.opacity_stack.clear();
        // Row-local frame: x stays, y maps the scanline to row 0.
        self.tx_stack
            .push(SkiaTransform::from_translate(0.0, -(y as f32)));
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
        // A chunk covering the scanline (half-open [top, bottom) in the
        // chunk's absolute bounds) paints into the row.
        for chunk in scene.chunks.iter().filter(|c| {
            (y as f32) >= c.bounds.y && (y as f32) < c.bounds.bottom()
        }) {
            let base = *ctx.tx_stack.last().unwrap();
            for clip in chunk.inherited_clips.iter() {
                ctx.clip_stack.push(
                    &clip.rect,
                    clip.radius,
                    base,
                    w,
                    1,
                    w,
                    1,
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
                &mut self.row.as_mut(),
                &mut ctx,
                w as f32,
                1.0,
            );
            ctx.tx_stack.pop();
            for _ in chunk.inherited_clips.iter() {
                ctx.clip_stack.pop();
            }
        }
        self.tx_stack.pop();
        self.clip_stack.reset();
        sink.push_line(y, self.row.data());
        Ok(())
    }
}

impl Default for LineBufferBackend {
    fn default() -> Self {
        Self::new(1, 1)
    }
}
