// Single responsibility: SceneSink implementation painting lowered primitives on TinySkia targets.
//
// No command matching, no stack walk, no placement policy: core lowers each
// chunk (`lower_chunk`) and calls one method per placed primitive. This file
// converts the absolute chunk-local transform to tiny-skia's type (composed
// with the engine root: tile offset) and delegates to the existing pixel
// routines.

use tiny_skia::{FillRule, Mask, PixmapMut, Transform as SkiaTransform};

use crate::clip::ClipStack;
use crate::image::ImageCache;
use crate::path::{build_rounded_path, skia_tx};
use crate::shader::build_paint;
use crate::shadow::ShadowRasterizer;
use crate::stroke::render_stroke;
use crate::svg::SvgCache;
use crate::text::{
    render_text_aligned, render_text_transformed, LineCompositeCache, TextRenderParams,
};
use riode::foundation::{Radius, Transform as CoreTransform};
use riode::scene::{
    PlacedClip, PlacedImage, PlacedRect, PlacedShadow, PlacedSvg, PlacedText, SceneSink,
    TextRaster,
};
use riode::text::TextContext;

/// Engine-side raster state for one chunk execution: pixel caches plus the
/// root device transform (tile offset) composed around every placed item.
pub struct ChunkSink<'a> {
    pub clip_stack: &'a mut ClipStack,
    pub text_ctx: &'a TextContext,
    pub svg_cache: &'a mut SvgCache,
    pub image_cache: &'a mut ImageCache,
    pub shadow_rasterizer: &'a mut ShadowRasterizer,
    pub line_cache: &'a mut LineCompositeCache,
    pub target: PixmapMut<'a>,
    /// Tile offset (device px). Composed with each item's absolute
    /// chunk-local transform at draw time — the only matrix the engine owns.
    pub root: SkiaTransform,
    pub tile_w: f32,
    pub tile_h: f32,
}

impl ChunkSink<'_> {
    /// Composes the engine root around a placed item's absolute transform.
    #[inline(always)]
    fn compose(&self, t: &CoreTransform) -> SkiaTransform {
        self.root.pre_concat(skia_tx(t))
    }

    #[inline(always)]
    fn target_size(&self) -> (u32, u32) {
        (self.target.width(), self.target.height())
    }
}

impl SceneSink for ChunkSink<'_> {
    fn push_clip(&mut self, clip: &PlacedClip) {
        let _span = tracing::info_span!("Raster::Clip").entered();
        let (mw, mh) = self.target_size();
        // Masks size to the scratch pixmap: tiny-skia silently skips draws
        // whose mask dimensions differ from the target.
        self.clip_stack.push(
            &clip.rect,
            clip.radius,
            self.compose(&clip.transform),
            self.tile_w as u32,
            self.tile_h as u32,
            mw,
            mh,
        );
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn rect(&mut self, r: &PlacedRect) {
        let _span = tracing::info_span!("Raster::Rect").entered();
        // Split borrow: the target pixmap vs the clip mask borrow.
        let tx = self.compose(&r.transform);
        let clip = self.clip_stack.current();
        render_rect(
            &mut self.target,
            &r.rect,
            r.appearance,
            r.opacity,
            tx,
            clip,
            self.tile_w,
            self.tile_h,
        );
    }

    fn text(&mut self, t: &PlacedText) {
        let _span = tracing::info_span!("Raster::Text").entered();
        let tx = self.compose(&t.transform);
        let clip = self.clip_stack.current();
        let params = TextRenderParams {
            origin: t.origin,
            layout: t.layout,
            layout_arc: Some(std::sync::Arc::clone(t.layout)),
            color: t.color,
            opacity: t.opacity,
            transform: tx,
            clip,
            line_cache: Some(&mut *self.line_cache),
        };
        // Raster path decided by core from the absolute transform.
        match t.raster {
            TextRaster::Aligned => render_text_aligned(&mut self.target, self.text_ctx, params),
            TextRaster::Transformed => {
                render_text_transformed(&mut self.target, self.text_ctx, params)
            }
        }
    }

    fn image(&mut self, i: &PlacedImage) {
        let tx = self.compose(&i.transform);
        let clip = self.clip_stack.current();
        self.image_cache.draw(
            &mut self.target,
            &i.rect,
            i.image,
            i.opacity,
            tx,
            clip,
            self.tile_w,
            self.tile_h,
        );
    }

    fn svg(&mut self, s: &PlacedSvg) {
        let (rw, rh) = riode::scene::raster_cache_size(s.rect.width, s.rect.height, 1.0);
        let tx = self.compose(&s.transform);
        let clip = self.clip_stack.current();
        self.svg_cache.draw(
            s.graphic,
            rw,
            rh,
            &mut self.target,
            s.opacity,
            tx.pre_translate(s.rect.x, s.rect.y),
            clip,
        );
    }

    fn shadow(&mut self, s: &PlacedShadow) {
        let _span = tracing::info_span!("Raster::Shadow").entered();
        let tx = self.compose(&s.transform);
        let clip = self.clip_stack.current();
        self.shadow_rasterizer.render(
            &mut self.target,
            &s.rect,
            s.radius,
            s.shadow,
            s.light,
            s.opacity,
            tx,
            clip,
        );
    }
}

/// Executes one chunk's commands onto a destination scratch pixmap tile.
///
/// `tile_tx` positions the tile in device px; the chunk base
/// (`translate(abs_origin) * placement`) is composed by core's lowering, so
/// the engine only ever sees placed items. Inherited clips (absolute
/// screen-space) are pushed with the tile transform directly, exactly as
/// before.
#[inline(always)]
pub fn execute_chunk(
    target: PixmapMut<'_>,
    tile_tx: SkiaTransform,
    chunk: &riode::scene::SceneChunk,
    ctx: &mut ChunkSinkContext<'_>,
) {
    for clip in chunk.inherited_clips.iter() {
        ctx.clip_stack.push(
            &clip.rect,
            clip.radius,
            tile_tx,
            ctx.tile_w as u32,
            ctx.tile_h as u32,
            target.width(),
            target.height(),
        );
    }
    {
        let mut sink = ChunkSink {
            clip_stack: &mut *ctx.clip_stack,
            text_ctx: ctx.text_ctx,
            svg_cache: &mut *ctx.svg_cache,
            image_cache: &mut *ctx.image_cache,
            shadow_rasterizer: &mut *ctx.shadow_rasterizer,
            line_cache: &mut *ctx.line_cache,
            target,
            root: tile_tx,
            tile_w: ctx.tile_w,
            tile_h: ctx.tile_h,
        };
        riode::scene::lower_chunk(
            &chunk.commands,
            &riode::scene::chunk_base(chunk),
            &mut sink,
        );
    }
    for _ in chunk.inherited_clips.iter() {
        ctx.clip_stack.pop();
    }
}

/// Shared pixel caches borrowed for one chunk execution. Split from
/// [`ChunkSink`] (which also owns the target pixmap and root) so callers
/// hold caches across chunks without moving the target.
pub struct ChunkSinkContext<'a> {
    pub clip_stack: &'a mut ClipStack,
    pub text_ctx: &'a TextContext,
    pub svg_cache: &'a mut SvgCache,
    pub image_cache: &'a mut ImageCache,
    pub shadow_rasterizer: &'a mut ShadowRasterizer,
    pub line_cache: &'a mut LineCompositeCache,
    pub tile_w: f32,
    pub tile_h: f32,
}

/// Tile culling honoring the full affine transform. The old test read only the
/// translation components, so rotated/scaled rects were culled against the
/// wrong box (missing pixels or wasted work). Axis-aligned transforms take a
/// cheap edge test; skewed ones project all four corners.
pub(crate) fn rect_visible_in_tile(
    r: &riode::foundation::ResolvedRect,
    tx: SkiaTransform,
    tw: f32,
    th: f32,
) -> bool {
    if !tx.has_skew() {
        let (x0, x1) = if tx.sx >= 0.0 {
            (r.x * tx.sx + tx.tx, (r.x + r.width) * tx.sx + tx.tx)
        } else {
            ((r.x + r.width) * tx.sx + tx.tx, r.x * tx.sx + tx.tx)
        };
        let (y0, y1) = if tx.sy >= 0.0 {
            (r.y * tx.sy + tx.ty, (r.y + r.height) * tx.sy + tx.ty)
        } else {
            ((r.y + r.height) * tx.sy + tx.ty, r.y * tx.sy + tx.ty)
        };
        return x1 > 0.0 && x0 < tw && y1 > 0.0 && y0 < th;
    }
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (px, py) in [
        (r.x, r.y),
        (r.x + r.width, r.y),
        (r.x, r.y + r.height),
        (r.x + r.width, r.y + r.height),
    ] {
        let x = tx.sx * px + tx.kx * py + tx.tx;
        let y = tx.ky * px + tx.sy * py + tx.ty;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    max_x > 0.0 && min_x < tw && max_y > 0.0 && min_y < th
}

fn render_rect(
    t: &mut PixmapMut,
    r: &riode::foundation::ResolvedRect,
    a: &riode::foundation::Appearance,
    op: f32,
    tx: SkiaTransform,
    clip: Option<&Mask>,
    tw: f32,
    th: f32,
) {
    if !rect_visible_in_tile(r, tx, tw, th) {
        return;
    }
    if let Some(fill) = &a.fill {
        if let Some(paint) = build_paint(fill, op * a.opacity) {
            if matches!(a.radius, Radius::Scalar(rad) if rad <= 0.0) {
                let (mut rx, mut ry, mut rw, mut rh) = (r.x, r.y, r.width, r.height);
                if !tx.has_skew() && !tx.has_scale() {
                    if rh <= 1.5 {
                        ry = ry.round();
                        rh = rh.round().max(1.0);
                    }
                    if rw <= 1.5 {
                        rx = rx.round();
                        rw = rw.round().max(1.0);
                    }
                }
                if let Some(sk) = tiny_skia::Rect::from_xywh(rx, ry, rw, rh) {
                    t.fill_rect(sk, &paint, tx, clip);
                }
            } else {
                t.fill_path(
                    &build_rounded_path(r, a.radius),
                    &paint,
                    FillRule::Winding,
                    tx,
                    clip,
                );
            }
        }
    }
    if let Some(stroke) = &a.stroke {
        render_stroke(t, r, a.radius, stroke, op * a.opacity, tx, clip);
    }
}
