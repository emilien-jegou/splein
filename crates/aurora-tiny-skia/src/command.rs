// Single responsibility: Direct dispatch and execution of discrete SceneCommands on TinySkia targets.

use tiny_skia::{FillRule, Mask, PixmapMut, Transform as SkiaTransform};

use aurora::foundation::Radius;
use crate::clip::ClipStack;
use crate::image::ImageCache;
use crate::path::build_rounded_path;
use crate::shader::build_paint;
use crate::shadow::ShadowRasterizer;
use crate::stroke::render_stroke;
use crate::svg::SvgCache;
use crate::text::{render_text, TextRenderParams};
use aurora::scene::command::SceneCommand;
use aurora::text::TextContext;

/// Shared stateful resources required during single-tile TinySkia rasterization.
pub struct CommandContext<'a> {
    pub clip_stack: &'a mut ClipStack,
    pub text_ctx: &'a TextContext,
    pub svg_cache: &'a mut SvgCache,
    pub image_cache: &'a mut ImageCache,
    pub shadow_rasterizer: &'a mut ShadowRasterizer,
    pub tx_stack: &'a mut Vec<SkiaTransform>,
    pub opacity_stack: &'a mut Vec<f32>,
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
    let (base_tx, base_op) = (ctx.tx_stack.len(), ctx.opacity_stack.len());
    ctx.tx_stack.push(
        ctx.tx_stack
            .last()
            .copied()
            .unwrap_or_else(SkiaTransform::identity),
    );
    ctx.opacity_stack
        .push(ctx.opacity_stack.last().copied().unwrap_or(1.0));

    for cmd in commands {
        let (cur_tx, cur_op) = (
            *ctx.tx_stack.last().unwrap(),
            *ctx.opacity_stack.last().unwrap(),
        );
        match cmd {
            SceneCommand::PushTransform(t) => {
                ctx.tx_stack.push(
                    cur_tx.pre_concat(SkiaTransform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty)),
                );
            }
            SceneCommand::PopTransform => {
                ctx.tx_stack.pop();
            }
            SceneCommand::PushOffset(p) => ctx.tx_stack.push(cur_tx.pre_translate(p.x, p.y)),
            SceneCommand::PopOffset => {
                ctx.tx_stack.pop();
            }
            SceneCommand::PushOpacity(o) => ctx.opacity_stack.push(cur_op * o.clamp(0.0, 1.0)),
            SceneCommand::PopOpacity => {
                ctx.opacity_stack.pop();
            }
            SceneCommand::PushClip { rect, radius } => {
                ctx.clip_stack
                    .push(rect, *radius, cur_tx, target.width(), target.height());
            }
            SceneCommand::PopClip => ctx.clip_stack.pop(),
            SceneCommand::DrawShadow {
                rect,
                radius,
                shadow,
            } => {
                ctx.shadow_rasterizer.render(
                    target,
                    rect,
                    *radius,
                    shadow,
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
                        color: *color,
                        opacity: cur_op,
                        transform: cur_tx,
                        clip: ctx.clip_stack.current(),
                    },
                );
            }
            // Isolates the subtree with a rectangular clip, matching the Vello
            // backend (push_layer/pop_layer). Previously this allocated an
            // offscreen pixmap but never redirected draws into it, so the
            // cache filled with empty bitmaps and isolation never happened.
            SceneCommand::BeginLayer { rect, .. } => {
                ctx.clip_stack
                    .push(rect, Radius::ZERO, cur_tx, target.width(), target.height());
            }
            SceneCommand::EndLayer { .. } => {
                ctx.clip_stack.pop();
            }
        }
    }
    ctx.tx_stack.truncate(base_tx);
    ctx.opacity_stack.truncate(base_op);
}

/// Tile culling honoring the full affine transform. The old test read only the
/// translation components, so rotated/scaled rects were culled against the
/// wrong box (missing pixels or wasted work). Axis-aligned transforms take a
/// cheap edge test; skewed ones project all four corners.
pub(crate) fn rect_visible_in_tile(
    r: &aurora::foundation::ResolvedRect,
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
    r: &aurora::foundation::ResolvedRect,
    a: &aurora::foundation::Appearance,
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
                if let Some(sk) = tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height) {
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
