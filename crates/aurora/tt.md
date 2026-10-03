Here are the three fixes to resolve these errors completely:

---

### 1. Delete the Obsolete Shadow File
You have both the old file and the new split directory. Delete the old file:
```bash
git rm crates/aurora/src/render/tiny_skia/shadow.rs
```

---

### 2. Full `crates/aurora/src/render/tiny_skia/command.rs` (132 lines)
The entire file was accidentally overwritten by a snippet. Here is the complete file:

```rust
// Single responsibility: Direct dispatch and execution of discrete SceneCommands on TinySkia targets.

use tiny_skia::{FillRule, Mask, Pixmap, PixmapMut, PixmapPaint, Transform as SkiaTransform};

use crate::foundation::Radius;
use crate::render::tiny_skia::clip::ClipStack;
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
    ctx.tx_stack.push(ctx.tx_stack.last().copied().unwrap_or_else(SkiaTransform::identity));
    ctx.opacity_stack.push(ctx.opacity_stack.last().copied().unwrap_or(1.0));

    for cmd in commands {
        let (cur_tx, cur_op) = (*ctx.tx_stack.last().unwrap(), *ctx.opacity_stack.last().unwrap());
        match cmd {
            SceneCommand::PushTransform(t) => {
                ctx.tx_stack.push(cur_tx.pre_concat(SkiaTransform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty)));
            }
            SceneCommand::PopTransform => { ctx.tx_stack.pop(); }
            SceneCommand::PushOffset(p) => ctx.tx_stack.push(cur_tx.pre_translate(p.x, p.y)),
            SceneCommand::PopOffset => { ctx.tx_stack.pop(); }
            SceneCommand::PushOpacity(o) => ctx.opacity_stack.push(cur_op * o.clamp(0.0, 1.0)),
            SceneCommand::PopOpacity => { ctx.opacity_stack.pop(); }
            SceneCommand::PushClip { rect, radius } => {
                ctx.clip_stack.push(rect, *radius, cur_tx, target.width(), target.height());
            }
            SceneCommand::PopClip => ctx.clip_stack.pop(),
            SceneCommand::DrawShadow { rect, radius, shadow } => {
                ctx.shadow_rasterizer.render(target, rect, *radius, shadow, cur_op, cur_tx, ctx.clip_stack.current());
            }
            SceneCommand::DrawRect { rect, appearance } => {
                render_rect(target, rect, appearance, cur_op, cur_tx, ctx.clip_stack.current(), tile_w, tile_h);
            }
            SceneCommand::DrawImage { rect, image } => {
                render_image(target, rect, image, cur_op, cur_tx, ctx.clip_stack.current(), tile_w, tile_h);
            }
            SceneCommand::DrawSvg { rect, graphic } => {
                ctx.svg_cache.draw(graphic, rect.width.round() as u32, rect.height.round() as u32, target, cur_op, cur_tx.pre_translate(rect.x, rect.y), ctx.clip_stack.current());
            }
            SceneCommand::DrawText { origin, layout, color } => {
                render_text(target, ctx.text_ctx, TextRenderParams { origin: *origin, layout, color: *color, opacity: cur_op, transform: cur_tx, clip: ctx.clip_stack.current() });
            }
            SceneCommand::BeginLayer { id, rect } => {
                let (lw, lh) = ((rect.width.ceil() as u32).max(1), (rect.height.ceil() as u32).max(1));
                if let Some(c) = ctx.layer_compositor.try_get_cached(*id, lw, lh) {
                    target.draw_pixmap(rect.x.round() as i32, rect.y.round() as i32, c.as_ref(), &PixmapPaint { opacity: cur_op, ..Default::default() }, cur_tx, ctx.clip_stack.current());
                } else if let Some(mut pm) = Pixmap::new(lw, lh) {
                    pm.fill(tiny_skia::Color::TRANSPARENT);
                    ctx.active_layers.push((*id, pm));
                }
            }
            SceneCommand::EndLayer { id } => {
                if let Some((lid, pm)) = ctx.active_layers.pop() {
                    if lid == *id {
                        target.draw_pixmap(0, 0, pm.as_ref(), &PixmapPaint { opacity: cur_op, ..Default::default() }, cur_tx, ctx.clip_stack.current());
                        ctx.layer_compositor.store(*id, pm);
                    }
                }
            }
        }
    }
    ctx.tx_stack.truncate(base_tx);
    ctx.opacity_stack.truncate(base_op);
}

fn render_rect(t: &mut PixmapMut, r: &crate::foundation::ResolvedRect, a: &crate::foundation::Appearance, op: f32, tx: SkiaTransform, clip: Option<&Mask>, tw: f32, th: f32) {
    if (r.x + tx.tx) >= tw || (r.x + tx.tx + r.width) <= 0.0 || (r.y + tx.ty) >= th || (r.y + tx.ty + r.height) <= 0.0 { return; }
    if let Some(fill) = &a.fill {
        if let Some(paint) = build_paint(fill, op * a.opacity) {
            if matches!(a.radius, Radius::Scalar(rad) if rad <= 0.0) {
                if let Some(sk) = tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height) { t.fill_rect(sk, &paint, tx, clip); }
            } else {
                t.fill_path(&build_rounded_path(r, a.radius), &paint, FillRule::Winding, tx, clip);
            }
        }
    }
    if let Some(stroke) = &a.stroke { render_stroke(t, r, a.radius, stroke, op * a.opacity, tx, clip); }
}

fn render_image(t: &mut PixmapMut, r: &crate::foundation::ResolvedRect, img: &crate::scene::image::ImageSource, op: f32, tx: SkiaTransform, clip: Option<&Mask>, tw: f32, th: f32) {
    if img.width == 0 || img.height == 0 || (r.x + tx.tx) >= tw || (r.y + tx.ty) >= th || (r.x + tx.tx + r.width) <= 0.0 || (r.y + tx.ty + r.height) <= 0.0 { return; }
    if let Some(pixmap) = Pixmap::from_vec(img.data.to_vec(), tiny_skia::IntSize::from_wh(img.width, img.height).unwrap()) {
        t.draw_pixmap(0, 0, pixmap.as_ref(), &PixmapPaint { opacity: op, ..Default::default() }, tx.pre_translate(r.x, r.y).pre_scale(r.width / img.width as f32, r.height / img.height as f32), clip);
    }
}
```

---

### 3. Full `crates/aurora/src/render/vello/shadow.rs` (120 lines)
The snippet was accidentally pasted at the top of the file. Here is the clean file with `Radius::Corners` properly handled in `draw`:

```rust
// Single responsibility: Analytical 9-patch blurred drop shadow rasterization and caching for Vello.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect};
use vello::peniko::{Blob, Brush, Fill as VelloFillRule, Format as PenikoFormat, Image as PenikoImage};
use vello::Scene as VelloScene;

use crate::foundation::{Radius, ResolvedRect, Shadow, ShadowKind};
use crate::render::blur::box_blur_alpha;
use crate::render::vello::shader::to_vello_color;

const MAX_CACHED_SHADOWS: usize = 64;

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
struct ShadowKey {
    radius_bits: u32,
    blur_bits: u32,
    spread_bits: u32,
    color_bits: u32,
}

struct ShadowPatch {
    corner: Arc<PenikoImage>,
    edge_h: Arc<PenikoImage>,
    edge_v: Arc<PenikoImage>,
    cs: f64,
}

/// Retained 9-patch texture cache for soft blurred drop shadows in Vello.
#[derive(Default)]
pub struct VelloShadowCache {
    cache: FxHashMap<ShadowKey, ShadowPatch>,
}

impl VelloShadowCache {
    /// Constructs a clean Vello shadow cache.
    pub fn new() -> Self {
        Self { cache: FxHashMap::default() }
    }

    /// Draws analytical 9-patch blurred drop shadows transformed through the affine stack.
    pub fn draw(&mut self, scene: &mut VelloScene, rect: &ResolvedRect, radius: Radius, shadow: &Shadow, tx: Affine) {
        if shadow.blur <= 0.0 && shadow.offset_x == 0.0 && shadow.offset_y == 0.0 || shadow.kind == ShadowKind::Inset {
            return;
        }

        let r_scalar = match radius {
            Radius::Scalar(v) => v.max(0.0),
            Radius::Max => (rect.width.min(rect.height) * 0.5).max(0.0),
            Radius::Corners { .. } => radius.resolve(rect.width, rect.height),
        };

        let blur_r = shadow.blur.max(0.5);
        let kernel_pad = (shadow.spread + 3.0 * blur_r).ceil();

        let key = ShadowKey {
            radius_bits: r_scalar.to_bits(),
            blur_bits: blur_r.to_bits(),
            spread_bits: shadow.spread.to_bits(),
            color_bits: (shadow.color.r.to_bits() ^ shadow.color.g.to_bits() ^ shadow.color.b.to_bits() ^ shadow.color.a.to_bits()),
        };

        if !self.cache.contains_key(&key) {
            if self.cache.len() >= MAX_CACHED_SHADOWS {
                if let Some(&first) = self.cache.keys().next() { self.cache.remove(&first); }
            }
            let patch = Self::generate_patch(r_scalar, blur_r, kernel_pad, shadow);
            self.cache.insert(key, patch);
        }

        let p = self.cache.get(&key).unwrap();
        let (ox, oy) = ((rect.x + shadow.offset_x - kernel_pad) as f64, (rect.y + shadow.offset_y - kernel_pad) as f64);
        let (total_w, total_h) = ((rect.width + 2.0 * kernel_pad) as f64, (rect.height + 2.0 * kernel_pad) as f64);
        let cs = p.cs;

        // 4 Corners
        scene.draw_image(p.corner.as_ref(), tx * Affine::translate((ox, oy)));
        scene.draw_image(p.corner.as_ref(), tx * Affine::translate((ox + total_w, oy)) * Affine::scale_non_uniform(-1.0, 1.0));
        scene.draw_image(p.corner.as_ref(), tx * Affine::translate((ox, oy + total_h)) * Affine::scale_non_uniform(1.0, -1.0));
        scene.draw_image(p.corner.as_ref(), tx * Affine::translate((ox + total_w, oy + total_h)) * Affine::scale_non_uniform(-1.0, -1.0));

        // Stretched Edges
        let edge_w = (total_w - 2.0 * cs).max(0.0);
        let edge_h = (total_h - 2.0 * cs).max(0.0);
        if edge_w > 0.0 {
            scene.draw_image(p.edge_h.as_ref(), tx * Affine::translate((ox + cs, oy)) * Affine::scale_non_uniform(edge_w, 1.0));
            scene.draw_image(p.edge_h.as_ref(), tx * Affine::translate((ox + cs, oy + total_h)) * Affine::scale_non_uniform(edge_w, -1.0));
        }
        if edge_h > 0.0 {
            scene.draw_image(p.edge_v.as_ref(), tx * Affine::translate((ox, oy + cs)) * Affine::scale_non_uniform(1.0, edge_h));
            scene.draw_image(p.edge_v.as_ref(), tx * Affine::translate((ox + total_w, oy + cs)) * Affine::scale_non_uniform(-1.0, edge_h));
        }

        // Center solid fill
        if edge_w > 0.0 && edge_h > 0.0 {
            let center_rect = KurboRect::new(ox + cs, oy + cs, ox + total_w - cs, oy + total_h - cs);
            scene.fill(VelloFillRule::NonZero, tx, &Brush::Solid(to_vello_color(shadow.color, 1.0)), None, &center_rect);
        }
    }

    fn generate_patch(radius: f32, blur: f32, pad: f32, shadow: &Shadow) -> ShadowPatch {
        let cs = ((radius + pad).ceil() as usize).max(4);
        let dim = cs * 2;
        let buf_len = dim * dim;
        let (mut a, mut b, mut temp) = (vec![0u8; buf_len], vec![0u8; buf_len], vec![0u8; buf_len]);

        for y in 0..dim {
            for x in 0..dim {
                let (px, py) = (x as f32, y as f32);
                if px >= pad && py >= pad { a[y * dim + x] = 255; }
            }
        }
        box_blur_alpha(&a, &mut b, &mut temp, dim, dim, blur.round() as usize);

        let (cr, cg, cb, ca) = ((shadow.color.r * 255.0).round() as u32, (shadow.color.g * 255.0).round() as u32, (shadow.color.b * 255.0).round() as u32, shadow.color.a.clamp(0.0, 1.0));
        let to_rgba = |alpha: u8| -> [u8; 4] {
            let a = ((alpha as f32 * ca).round() as u32).min(255);
            [((cr * a + 128) >> 8) as u8, ((cg * a + 128) >> 8) as u8, ((cb * a + 128) >> 8) as u8, a as u8]
        };

        let mut corner_bytes = Vec::with_capacity(cs * cs * 4);
        for y in 0..cs { for x in 0..cs { corner_bytes.extend_from_slice(&to_rgba(b[y * dim + x])); } }

        let mut edge_h_bytes = Vec::with_capacity(cs * 4);
        for y in 0..cs { edge_h_bytes.extend_from_slice(&to_rgba(b[y * dim + cs])); }

        let mut edge_v_bytes = Vec::with_capacity(cs * 4);
        for x in 0..cs { edge_v_bytes.extend_from_slice(&to_rgba(b[cs * dim + x])); }

        ShadowPatch {
            corner: Arc::new(PenikoImage::new(Blob::from(corner_bytes), PenikoFormat::Rgba8, cs as u32, cs as u32)),
            edge_h: Arc::new(PenikoImage::new(Blob::from(edge_h_bytes), PenikoFormat::Rgba8, 1, cs as u32)),
            edge_v: Arc::new(PenikoImage::new(Blob::from(edge_v_bytes), PenikoFormat::Rgba8, cs as u32, 1)),
            cs: cs as f64,
        }
    }
}
```
