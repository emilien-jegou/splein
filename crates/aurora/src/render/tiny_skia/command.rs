// Single responsibility: Direct dispatch and execution of discrete SceneCommands on TinySkia targets.

use tiny_skia::{FillRule, Mask, Pixmap, PixmapMut, PixmapPaint, Transform as SkiaTransform};

use crate::foundation::Radius;
use crate::render::tiny_skia::clip::ClipStack;
use crate::render::tiny_skia::image::ImageCache;
use crate::render::tiny_skia::layer::LayerCompositor;
use crate::render::tiny_skia::path::build_rounded_path;
use crate::render::tiny_skia::shader::build_paint;
use crate::render::tiny_skia::shadow::ShadowRasterizer;
use crate::render::tiny_skia::stroke::render_stroke;
use crate::render::tiny_skia::svg::SvgCache;
use crate::render::tiny_skia::text::{render_text, TextRenderParams};
use crate::scene::command::{LayerId, SceneCommand};
use crate::text::TextContext;

/// Shared stateful resources required during single-tile TinySkia rasterization.
pub struct CommandContext<'a> {
    pub clip_stack: &'a mut ClipStack,
    pub text_ctx: &'a TextContext,
    pub svg_cache: &'a mut SvgCache,
    pub image_cache: &'a mut ImageCache,
    pub shadow_rasterizer: &'a mut ShadowRasterizer,
    pub layer_compositor: &'a mut LayerCompositor,
    pub active_layers: &'a mut Vec<(LayerId, Pixmap)>,
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
            SceneCommand::BeginLayer { id, rect } => {
                let (lw, lh) = (
                    (rect.width.ceil() as u32).max(1),
                    (rect.height.ceil() as u32).max(1),
                );
                if let Some(c) = ctx.layer_compositor.try_get_cached(*id, lw, lh) {
                    target.draw_pixmap(
                        rect.x.round() as i32,
                        rect.y.round() as i32,
                        c.as_ref(),
                        &PixmapPaint {
                            opacity: cur_op,
                            ..Default::default()
                        },
                        cur_tx,
                        ctx.clip_stack.current(),
                    );
                } else if let Some(mut pm) = Pixmap::new(lw, lh) {
                    pm.fill(tiny_skia::Color::TRANSPARENT);
                    ctx.active_layers.push((*id, pm));
                }
            }
            SceneCommand::EndLayer { id } => {
                if let Some((lid, pm)) = ctx.active_layers.pop() {
                    if lid == *id {
                        target.draw_pixmap(
                            0,
                            0,
                            pm.as_ref(),
                            &PixmapPaint {
                                opacity: cur_op,
                                ..Default::default()
                            },
                            cur_tx,
                            ctx.clip_stack.current(),
                        );
                        ctx.layer_compositor.store(*id, pm);
                    }
                }
            }
        }
    }
    ctx.tx_stack.truncate(base_tx);
    ctx.opacity_stack.truncate(base_op);
}

fn render_rect(
    t: &mut PixmapMut,
    r: &crate::foundation::ResolvedRect,
    a: &crate::foundation::Appearance,
    op: f32,
    tx: SkiaTransform,
    clip: Option<&Mask>,
    tw: f32,
    th: f32,
) {
    if (r.x + tx.tx) >= tw
        || (r.x + tx.tx + r.width) <= 0.0
        || (r.y + tx.ty) >= th
        || (r.y + tx.ty + r.height) <= 0.0
    {
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
