// Single responsibility: SceneSink implementation painting lowered primitives on TinySkia targets.
//
// No command matching, no stack walk, no placement policy: core lowers each
// chunk (`lower_chunk`) and calls one method per placed primitive. This file
// converts the absolute chunk-local transform to tiny-skia's type (composed
// with the engine root: tile offset) and delegates to the existing pixel
// routines.

use tiny_skia::{FillRule, Mask, PixmapMut, PixmapPaint, Transform as SkiaTransform};

use crate::clip::ClipStack;
use crate::filter::LayerFrame;
use crate::image::ImageCache;
use crate::path::{build_rounded_path, skia_tx};
use crate::shader::build_paint;
use crate::shadow::ShadowRasterizer;
use crate::stroke::render_stroke;
use crate::svg::SvgCache;
use crate::text::{
    render_text_aligned, render_text_transformed, LineCompositeCache, TextRenderParams,
};
use riode::foundation::{Radius, Transform as CoreTransform};use riode::scene::{
    PlacedClip, PlacedImage, PlacedLayer, PlacedRect, PlacedShadow, PlacedSvg, PlacedText,
    SceneSink, TextRaster,
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
    /// Open filter layers. Draws route to the top layer's offscreen when it
    /// owns a pixmap; a `pixmap: None` entry is a plain clip layer (the
    /// pre-filter `has_layer` behavior).
    pub layers: Vec<LayerFrame>,
}

// Glass is not implemented in this engine: a `BeginLayer { glass: Some(..) }`
// is a plain isolation layer here, and the backdrop effect belongs to the
// wgpu path (or to the compositor plugin). See `ongoing-liquid-glass-upgrade.md`.

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
        let composed = self.compose(&clip.transform);
        // Inside a filter layer, size the mask to the offscreen and intersect
        // only in-layer masks: tile-sized ancestor masks must not mix with
        // offscreen-sized ones (the zip would misalign).
        if let Some(frame) = self.layers.last_mut() {
            let (ox, oy) = frame.origin;
            let floor = frame.clip_floor;
            if let Some(pm) = frame.pixmap.as_mut() {
                let (mw, mh) = (pm.width(), pm.height());
                let tx = tiny_skia::Transform::from_translate(-ox, -oy).pre_concat(composed);
                self.clip_stack
                    .push_from(floor, &clip.rect, clip.radius, tx, mw, mh, mw, mh);
                return;
            }
        }
        let (mw, mh) = self.target_size();
        // Masks size to the scratch pixmap: tiny-skia silently skips draws
        // whose mask dimensions differ from the target.
        self.clip_stack.push(
            &clip.rect,
            clip.radius,
            composed,
            self.tile_w as u32,
            self.tile_h as u32,
            mw,
            mh,
        );
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn begin_layer(&mut self, layer: &PlacedLayer) {
        let _span = tracing::info_span!("Raster::Layer").entered();
        // Backdrop effects read the parent pixels — this engine samples no
        // backdrop (no glass, no blend), so the stack is ignored here.
        if layer.backdrop_effects.is_some() {
            use std::sync::atomic::{AtomicBool, Ordering};
            static WARNED: AtomicBool = AtomicBool::new(false);
            if !WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "riode-tiny-skia: backdrop_effects ignored (this engine samples no backdrop; use wgpu)"
                );
            }
        }
        let floor = self.clip_stack.depth();
        // Effect-stack lowering: the sheet's stack (`layer_effects`)
        // lowers to blur sigmas as `Blur` ops and color stages as raw
        // `Matrix` ops. Glass lives on the backdrop side and draws no
        // glass on this engine (see the note at the top of this file),
        // so a glass-only layer stays isolation-free, exactly like a
        // pre-glass frame. A plain layer is a clip — unless filter-like
        // stages or the alpha-shadow pad demand an offscreen.
        let filter = layer.layer_effects.as_ref().and_then(|s| s.to_filter());
        if filter.is_none() && layer.shadow_pad <= 0.0 {
            // Plain isolation layer (pre-filter behavior): a clip.
            self.push_clip(&PlacedClip {
                rect: layer.rect,
                radius: Radius::ZERO,
                transform: layer.transform,
            });
            self.layers.push(LayerFrame {
                pixmap: None,
                origin: (0.0, 0.0),
                filter,
                clip_floor: floor,
                alpha_shadow: None,
            });
            return;
        };
        // Device-space AABB of the layer rect under its placed transform.
        let tx = self.compose(&layer.transform);
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (px, py) in [
            (layer.rect.x, layer.rect.y),
            (layer.rect.x + layer.rect.width, layer.rect.y),
            (layer.rect.x, layer.rect.y + layer.rect.height),
            (layer.rect.x + layer.rect.width, layer.rect.y + layer.rect.height),
        ] {
            let x = tx.sx * px + tx.kx * py + tx.tx;
            let y = tx.ky * px + tx.sy * py + tx.ty;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        // Gaussian support: 3σ past the rect so blur tails stay complete,
        // plus the alpha-shadow pad.
        let sigma = filter.as_ref().map(|f| f.max_blur_sigma()).unwrap_or(0.0);
        let pad = 3.0 * sigma + layer.shadow_pad;
        min_x -= pad;
        min_y -= pad;
        max_x += pad;
        max_y += pad;
        // Clamp to the tile neighborhood: only what this tile can show plus
        // the blur's source margin. Bounds memory to ~tile + 6σ per axis.
        min_x = min_x.max(-pad);
        min_y = min_y.max(-pad);
        max_x = max_x.min(self.tile_w + pad);
        max_y = max_y.min(self.tile_h + pad);
        let (ox, oy) = (min_x.floor(), min_y.floor());
        // At least 1×1 so a fully off-tile layer still owns an offscreen
        // (leaking its draws unfiltered onto the tile would be wrong).
        let cap_w = (self.tile_w + 2.0 * pad + 2.0).max(1.0) as u32;
        let cap_h = (self.tile_h + 2.0 * pad + 2.0).max(1.0) as u32;
        let w = ((max_x.ceil() - ox).max(1.0) as u32).min(cap_w);
        let h = ((max_y.ceil() - oy).max(1.0) as u32).min(cap_h);
        let pixmap = tiny_skia::Pixmap::new(w, h);

        self.layers.push(LayerFrame {
            pixmap,
            origin: (ox, oy),
            filter,
            clip_floor: floor,
            alpha_shadow: None,
        });
    }

    fn end_layer(&mut self) {
        let _span = tracing::info_span!("Raster::LayerEnd").entered();
        let Some(frame) = self.layers.pop() else {
            tracing::warn!("layer stack underflow: EndLayer without a matching BeginLayer");
            self.pop_clip();
            return;
        };
        let crate::filter::LayerFrame {
            pixmap,
            origin,
            filter,
            clip_floor,
            alpha_shadow,
        } = frame;
        let Some(mut pm) = pixmap else {
            // Clip layer: the clip pushed in begin_layer is popped here.
            self.clip_stack.truncate_to(clip_floor);
            self.pop_clip();
            return;
        };
        // In-layer clips die with the layer; they must not mask the composite.
        self.clip_stack.truncate_to(clip_floor);
        if let Some(filter) = &filter {
            crate::filter::apply_filter(&mut pm, filter);
        }
        // Fields used piecewise: the alpha-shadow pass and the content
        // composite each borrow the sink exclusively, never together.
        let ChunkSink {
            clip_stack,
            shadow_rasterizer,
            root,
            target,
            layers,
            ..
        } = self;
        let parent = layers.iter().rposition(|f| f.pixmap.is_some());
        // Alpha drop shadow first: it lands on the parent *behind* the
        // content composite that follows, cast from the finished silhouette
        // (post-filter, in-layer clips baked in).
        if let Some(a) = &alpha_shadow {
            let composed = root.pre_concat(skia_tx(&a.transform));
            if let Some(i) = parent {
                let (pox, poy) = layers[i].origin;
                let pfloor = layers[i].clip_floor;
                let clip = clip_stack.current_from(pfloor);
                if let Some(dst) = layers[i].pixmap.as_mut() {
                    shadow_rasterizer.render_alpha(
                        &pm,
                        &mut dst.as_mut(),
                        (origin.0 - pox, origin.1 - poy),
                        &a.shadow,
                        &a.light,
                        a.opacity,
                        composed,
                        clip,
                    );
                }
            } else {
                let clip = clip_stack.current();
                shadow_rasterizer.render_alpha(
                    &pm,
                    target,
                    origin,
                    &a.shadow,
                    &a.light,
                    a.opacity,
                    composed,
                    clip,
                );
            }
        }
        // Composite onto the parent surface.
        let paint = PixmapPaint::default();
        let ts = SkiaTransform::identity();
        if let Some(i) = parent {
            let (pox, poy) = layers[i].origin;
            let clip = clip_stack.current_from(layers[i].clip_floor);
            let (ppx, ppy) = (
                (origin.0 - pox).round() as i32,
                (origin.1 - poy).round() as i32,
            );
            if let Some(dst) = layers[i].pixmap.as_mut() {
                dst.draw_pixmap(ppx, ppy, pm.as_ref(), &paint, ts, clip);
            }
        } else {
            let clip = clip_stack.current();
            let (px, py) = (origin.0.round() as i32, origin.1.round() as i32);
            target.draw_pixmap(px, py, pm.as_ref(), &paint, ts, clip);
        }
    }

    fn rect(&mut self, r: &PlacedRect) {
        let _span = tracing::info_span!("Raster::Rect").entered();
        let tx = self.compose(&r.transform);
        // Route into the active filter offscreen when one is open.
        if let Some(frame) = self.layers.last_mut() {
            let (ox, oy) = frame.origin;
            let floor = frame.clip_floor;
            if let Some(pm) = frame.pixmap.as_mut() {
                let tw = pm.width() as f32;
                let th = pm.height() as f32;
                let tx = tiny_skia::Transform::from_translate(-ox, -oy).pre_concat(tx);
                let clip = self.clip_stack.current_from(floor);
                render_rect(&mut pm.as_mut(), &r.rect, r.appearance, r.opacity, tx, clip, tw, th);
                return;
            }
        }
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
        if let Some(frame) = self.layers.last_mut() {
            let (ox, oy) = frame.origin;
            let floor = frame.clip_floor;
            if let Some(pm) = frame.pixmap.as_mut() {
                let tx = tiny_skia::Transform::from_translate(-ox, -oy).pre_concat(tx);
                let clip = self.clip_stack.current_from(floor);
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
                match t.raster {
                    TextRaster::Aligned => {
                        render_text_aligned(&mut pm.as_mut(), self.text_ctx, params)
                    }
                    TextRaster::Transformed => {
                        render_text_transformed(&mut pm.as_mut(), self.text_ctx, params)
                    }
                }
                return;
            }
        }
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
        if let Some(frame) = self.layers.last_mut() {
            let (ox, oy) = frame.origin;
            let floor = frame.clip_floor;
            if let Some(pm) = frame.pixmap.as_mut() {
                let tw = pm.width() as f32;
                let th = pm.height() as f32;
                let tx = tiny_skia::Transform::from_translate(-ox, -oy).pre_concat(tx);
                let clip = self.clip_stack.current_from(floor);
                self.image_cache
                    .draw(&mut pm.as_mut(), &i.rect, i.image, i.opacity, tx, clip, tw, th);
                return;
            }
        }
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
        if let Some(frame) = self.layers.last_mut() {
            let (ox, oy) = frame.origin;
            let floor = frame.clip_floor;
            if let Some(pm) = frame.pixmap.as_mut() {
                let tx = tiny_skia::Transform::from_translate(-ox, -oy).pre_concat(tx);
                let clip = self.clip_stack.current_from(floor);
                self.svg_cache.draw(
                    s.graphic,
                    rw,
                    rh,
                    &mut pm.as_mut(),
                    s.opacity,
                    tx.pre_translate(s.rect.x, s.rect.y),
                    clip,
                );
                return;
            }
        }
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
        // Alpha silhouette inside its own offscreen layer: stash instead of
        // painting — `end_layer` casts it from the finished layer alpha,
        // behind the content composite.
        if matches!(s.shadow.shape, riode::foundation::ShadowShape::Alpha)
            && !s.shadow.inset
        {
            if let Some(frame) = self.layers.last_mut().filter(|f| f.pixmap.is_some()) {
                frame.alpha_shadow = Some(crate::filter::AlphaShadow {
                    shadow: s.shadow.clone(),
                    light: *s.light,
                    opacity: s.opacity,
                    transform: s.transform,
                });
                return;
            }
        }
        let tx = self.compose(&s.transform);
        if let Some(frame) = self.layers.last_mut() {
            let (ox, oy) = frame.origin;
            let floor = frame.clip_floor;
            if let Some(pm) = frame.pixmap.as_mut() {
                let tx = tiny_skia::Transform::from_translate(-ox, -oy).pre_concat(tx);
                let clip = self.clip_stack.current_from(floor);
                self.shadow_rasterizer
                    .render(&mut pm.as_mut(), &s.rect, s.radius, s.shadow, s.light, s.opacity, tx, clip);
                return;
            }
        }
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
            layers: Vec::new(),
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
