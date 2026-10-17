// Single responsibility: Direct dispatch and execution of discrete SceneCommands on TinySkia targets.

use tiny_skia::{FillRule, Mask, PixmapMut, Transform as SkiaTransform};

use crate::clip::ClipStack;
use crate::image::ImageCache;
use crate::path::{build_rounded_path, skia_tx};
use crate::shader::build_paint;
use crate::shadow::ShadowRasterizer;
use crate::stroke::render_stroke;
use crate::svg::SvgCache;
use crate::text::{render_text, LineCompositeCache, TextRenderParams};
use riode::foundation::Radius;
use riode::scene::command::SceneCommand;
use riode::scene::SceneStack;
use riode::text::TextContext;

/// Shared stateful resources required during single-tile TinySkia rasterization.
pub struct CommandContext<'a> {
    pub clip_stack: &'a mut ClipStack,
    pub text_ctx: &'a TextContext,
    pub svg_cache: &'a mut SvgCache,
    pub image_cache: &'a mut ImageCache,
    pub shadow_rasterizer: &'a mut ShadowRasterizer,
    /// Framing-state walk shared with all backends (core semantics).
    pub stack: &'a mut SceneStack,
    pub line_cache: &'a mut LineCompositeCache,
}

/// Executes a list of scene commands onto a destination scratch pixmap tile.
#[inline(always)]
pub fn execute_commands(
    commands: &[SceneCommand],
    target: &mut PixmapMut,
    ctx: &mut CommandContext,
    tile_w: f32,
    tile_h: f32,
) {
    let checkpoint = ctx.stack.checkpoint();
    // Seed the base frame so every chunk starts from the inherited state,
    // mirroring the old push-identity/push-base sequence.
    ctx.stack.push_transform(&riode::foundation::Transform::IDENTITY);

    for cmd in commands {
        // Backend transform converted from the shared core state at draw
        // time; the stack walk itself is backend-agnostic.
        let (cur_tx, cur_op) = (skia_tx(&ctx.stack.transform()), ctx.stack.opacity());
        match cmd {
            SceneCommand::PushTransform(t) => {
                ctx.stack.push_transform(t);
            }
            SceneCommand::PopTransform => {
                ctx.stack.pop_transform();
            }
            SceneCommand::PushOffset(p) => ctx.stack.push_offset(*p),
            SceneCommand::PopOffset => {
                ctx.stack.pop_transform();
            }
            SceneCommand::PushOpacity(o) => ctx.stack.push_opacity(*o),
            SceneCommand::PopOpacity => {
                ctx.stack.pop_opacity();
            }
            SceneCommand::PushClip { rect, radius } => {
                ctx.clip_stack.push(
                    rect,
                    *radius,
                    cur_tx,
                    tile_w as u32,
                    tile_h as u32,
                    target.width(),
                    target.height(),
                );
            }
            SceneCommand::PopClip => ctx.clip_stack.pop(),
            SceneCommand::DrawShadow {
                rect,
                radius,
                shadow,
                light,
            } => {
                ctx.shadow_rasterizer.render(
                    target,
                    rect,
                    *radius,
                    shadow,
                    light,
                    cur_op,
                    cur_tx,
                    ctx.clip_stack.current(),
                );
            }
            SceneCommand::DrawRect { rect, appearance } => {
                render_rect(
                    target,
                    rect,
                    appearance,
                    cur_op,
                    cur_tx,
                    ctx.clip_stack.current(),
                    tile_w,
                    tile_h,
                );
            }
            SceneCommand::DrawImage { rect, image } => {
                ctx.image_cache.draw(
                    target,
                    rect,
                    image,
                    cur_op,
                    cur_tx,
                    ctx.clip_stack.current(),
                    tile_w,
                    tile_h,
                );
            }
            SceneCommand::DrawSvg { rect, graphic } => {
                ctx.svg_cache.draw(
                    graphic,
                    rect.width.round() as u32,
                    rect.height.round() as u32,
                    target,
                    cur_op,
                    cur_tx.pre_translate(rect.x, rect.y),
                    ctx.clip_stack.current(),
                );
            }
            SceneCommand::DrawText {
                origin,
                layout,
                color,
            } => {
                render_text(
                    target,
                    ctx.text_ctx,
                    TextRenderParams {
                        origin: *origin,
                        layout,
                        layout_arc: Some(std::sync::Arc::clone(layout)),
                        color: *color,
                        opacity: cur_op,
                        transform: cur_tx,
                        clip: ctx.clip_stack.current(),
                        line_cache: Some(&mut *ctx.line_cache),
                    },
                );
            }
            // Isolates the subtree with a rectangular clip, matching the Vello
            // backend (push_layer/pop_layer). Previously this allocated an
            // offscreen pixmap but never redirected draws into it, so the
            // cache filled with empty bitmaps and isolation never happened.
            SceneCommand::BeginLayer { rect, .. } => {
                ctx.clip_stack.push(
                    rect,
                    Radius::ZERO,
                    cur_tx,
                    tile_w as u32,
                    tile_h as u32,
                    target.width(),
                    target.height(),
                );
            }
            SceneCommand::EndLayer { .. } => {
                ctx.clip_stack.pop();
            }
        }
    }
    ctx.stack.restore(checkpoint);
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
    // Snap the shared translation (not each rect): box fill, highlight and
    // any other rect under this transform shift by the same amount, so
    // coincident edges stay coincident and land integer together.
    let tx = crate::path::snap_transform_to_device(tx);
    if let Some(fill) = &a.fill {
        if let Some(paint) = build_paint(fill, op * a.opacity) {
            // Device snap first: translation-only fractional offsets fringe
            // every edge; then the hairline rule below keeps thin rects >=1px.
            let sr = crate::path::snap_rect_to_device(r, tx);
            if matches!(a.radius, Radius::Scalar(rad) if rad <= 0.0) {
                // 1px hairlines at fractional positions split coverage over
                // two rows (full-width faint double line). Snap thin
                // translation-only rects to integer pixels so they stay crisp.
                let (mut rx, mut ry, mut rw, mut rh) = (sr.x, sr.y, sr.width, sr.height);
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
                    &build_rounded_path(&sr, a.radius),
                    &paint,
                    FillRule::Winding,
                    tx,
                    clip,
                );
            }
        }
    }
    if let Some(stroke) = &a.stroke {
        let sr = crate::path::snap_rect_to_device(r, tx);
        render_stroke(t, &sr, a.radius, stroke, op * a.opacity, tx, clip);
    }
}
