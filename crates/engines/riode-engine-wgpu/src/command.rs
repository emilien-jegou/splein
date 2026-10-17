// Single responsibility: SceneCommand -> batched rect + glyph vertex emission.

use riode::foundation::{
    Color, Fill, LightEnvironment, Point, Radius, ResolvedRect, Shadow, ShadowShape,
    StrokeAlign, Transform,
};
use riode::scene::command::SceneCommand;
use riode::scene::{
    quantize_eighth, quantize_quarter, PlacedClip, PlacedImage, PlacedRect, PlacedShadow,
    PlacedSvg, PlacedText, SceneSink,
};
use riode::text::{quantize_raster_scale, TextContext};
use std::sync::Arc;

use crate::clip::ClipStack;
use crate::image::ImageGpu;
use crate::pipeline::{GlyphVertex, ImageVertex, RRectVertex, RectVertex, ShadowVertex};
use crate::text::GlyphAtlas;

static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WARNED_MULTIPLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn warn_once(msg: &str) {
    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!("riode-wgpu: {msg} (further skips silent)");
    }
}

/// Separate flag so the blend approximation isn't silenced by an
/// earlier unrelated skip warning (elevation shadows are common).
fn warn_multiply_once() {
    if !WARNED_MULTIPLY.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!(
            "riode-wgpu: Shadow BlendMode::Multiply approximated as Normal (further silent)"
        );
    }
}

/// Exact sRGB EOTF (encoded -> linear), for the Oklab conversion below
/// (the LMS matrices take linear RGB).
fn srgb_to_linear1(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB (encoded) -> Oklab (Bjorn Ottosson). Linear-ized first: the LMS
/// matrices take linear RGB. `cbrt` is sign-preserving, so the (always
/// non-negative) LMS cube roots are exact.
fn srgb_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let r = srgb_to_linear1(c[0]);
    let g = srgb_to_linear1(c[1]);
    let b = srgb_to_linear1(c[2]);
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

/// One draw op over the rect, glyph, rounded-rect, or shadow vertex buffers.
#[derive(Clone, Copy)]
pub enum Op {
    Rects {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
    },
    RRects {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
    },
    Shadows {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
    },
    /// Opaque base-color quads repainting the damaged area on partial
    /// frames. Always emitted first; drawn with blending disabled so they
    /// replace stale pixels instead of blending over them.
    BaseQuads { start: u32, count: u32 },
    /// One textured quad; `bind` indexes the batch's `image_binds`
    /// (each image has its own texture, so ops never merge).
    Images {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
        bind: u32,
    },
    Glyphs {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
        /// True when the current transform needs bilinear filtering
        /// (scale/skew/rotation); translation-only uses nearest.
        linear: bool,
    },
}

/// Emission context: batched verts + ordered ops.
///
/// No framing stack: core lowers each chunk into placed items
/// (`SceneSink`), and every item already carries its absolute transform and
/// folded opacity. The engine composes its root device scale around each
/// item — the only matrix it owns.
pub struct Batch {
    pub clip: ClipStack,
    pub rect_verts: Vec<RectVertex>,
    pub rrect_verts: Vec<RRectVertex>,
    pub shadow_verts: Vec<ShadowVertex>,
    pub glyph_verts: Vec<GlyphVertex>,
    pub image_verts: Vec<ImageVertex>,
    /// GPU textures referenced by `Op::Images.bind` indices. Arc-held so
    /// a cached chunk's replayed ops stay valid past LRU eviction from
    /// the texture cache (the Arc is the refcount).
    pub image_binds: Vec<Arc<ImageGpu>>,
    pub ops: Vec<Op>,
    /// Device scale: layout px -> device px. Baked as the root transform
    /// so all command coords land in device pixels; bin math then operates
    /// in device space (GPUI parity). Coverage bitmaps stay layout-res
    /// (engine shapes at logical size) — right size, softer at dsf>1
    /// until the engine rasters at size*dsf.
    scale: f32,
    /// Chunk-boundary barrier: set at the start of each chunk's emission so
    /// the next run push starts a fresh op instead of extending the previous
    /// chunk's run. Without this, per-chunk cache snapshots slice verts and
    /// ops that no longer correspond (a chunk whose draws all merged into
    /// the previous chunk's run snapshots zero ops and replays invisible).
    barrier: bool,
}

impl Batch {
    pub fn new() -> Self {
        Self {
            clip: ClipStack::default(),
            rect_verts: Vec::with_capacity(1024),
            rrect_verts: Vec::with_capacity(256),
            shadow_verts: Vec::with_capacity(256),
            glyph_verts: Vec::with_capacity(1024),
            image_verts: Vec::with_capacity(256),
            image_binds: Vec::new(),
            ops: Vec::new(),
            scale: 1.0,
            barrier: false,
        }
    }

    pub fn set_scale(&mut self, dsf: f32) {
        self.scale = dsf.clamp(0.5, 4.0);
    }

    /// Device scale baked into emitted verts; the vertex cache keys replay
    /// validity off it (a scale change re-emits, never rebases).
    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn reset(&mut self) {
        self.clip.reset();
        self.rect_verts.clear();
        self.rrect_verts.clear();
        self.shadow_verts.clear();
        self.glyph_verts.clear();
        self.image_verts.clear();
        self.image_binds.clear();
        self.ops.clear();
        self.barrier = false;
    }

    /// Marks the start of a chunk's emission: the next run of any kind
    /// starts a fresh op. Called by the renderer per chunk (after base
    /// quads, before clips/base/commands) so snapshots stay complete.
    pub fn begin_chunk(&mut self) {
        self.barrier = true;
    }

    /// Root device matrix: layout px -> device px. Composed around every
    /// placed item's absolute transform — the only matrix the engine owns.
    fn device_tx(&self, t: &Transform) -> Transform {
        Transform::from_scale(self.scale, self.scale).multiply(t)
    }

    /// Pushes an already-absolute layout-space clip as device pixels.
    /// For inherited clips, which arrive in absolute screen space.
    pub fn push_clip_device(&mut self, rect: &ResolvedRect, radius: Radius) {
        let s = self.scale;
        self.clip.push(
            &ResolvedRect::new(rect.x * s, rect.y * s, rect.width * s, rect.height * s),
            radius,
        );
    }

    /// Pushes a node-local clip rect, transformed by the composed device
    /// matrix into device pixels (outward-rounded AABB: over-covers under
    /// skew, which clips less rather than more).
    /// For command-stream clips, which are relative to the current frame
    /// (node offsets precede the clip in the stream). Ignoring the
    /// transform intersects the rect near the origin and silently drops
    /// every op of nodes placed far from it.
    pub fn push_clip_local(&mut self, rect: &ResolvedRect, radius: Radius, t: Transform) {
        let mut x0 = f32::INFINITY;
        let mut y0 = f32::INFINITY;
        let mut x1 = f32::NEG_INFINITY;
        let mut y1 = f32::NEG_INFINITY;
        for (x, y) in [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x, rect.y + rect.height),
            (rect.x + rect.width, rect.y + rect.height),
        ] {
            let p = t.transform_point(Point::new(x, y));
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        self.clip.push(
            &riode::scene::integerize_damage(&ResolvedRect::new(x0, y0, x1 - x0, y1 - y0)),
            radius,
        );
    }

    fn push_rect_run(&mut self, count: u32) {
        let sc = self.clip.current();
        // A chunk boundary always starts a fresh op (see begin_chunk), so
        // per-chunk snapshots stay complete even when the scissor matches
        // the previous chunk's run.
        let fresh = std::mem::replace(&mut self.barrier, false);
        let extend = match self.ops.last_mut() {
            Some(Op::Rects {
                count: c,
                scissor: s,
                ..
            }) if *s == sc && !fresh => Some(c),
            _ => None,
        };
        match extend {
            Some(c) => *c += count,
            None => self.ops.push(Op::Rects {
                start: self.rect_verts.len() as u32 - count,
                count,
                scissor: sc,
            }),
        }
    }

    fn push_rrect_run(&mut self, count: u32) {
        let sc = self.clip.current();
        let fresh = std::mem::replace(&mut self.barrier, false);
        let extend = match self.ops.last_mut() {
            Some(Op::RRects {
                count: c,
                scissor: s,
                ..
            }) if *s == sc && !fresh => Some(c),
            _ => None,
        };
        match extend {
            Some(c) => *c += count,
            None => self.ops.push(Op::RRects {
                start: self.rrect_verts.len() as u32 - count,
                count,
                scissor: sc,
            }),
        }
    }

    fn push_shadow_run(&mut self, count: u32) {
        let sc = self.clip.current();
        let fresh = std::mem::replace(&mut self.barrier, false);
        let extend = match self.ops.last_mut() {
            Some(Op::Shadows {
                count: c,
                scissor: s,
                ..
            }) if *s == sc && !fresh => Some(c),
            _ => None,
        };
        match extend {
            Some(c) => *c += count,
            None => self.ops.push(Op::Shadows {
                start: self.shadow_verts.len() as u32 - count,
                count,
                scissor: sc,
            }),
        }
    }

    /// Translation + uniform positive scale (within epsilon): the SDF box
    /// stays axis-aligned and radii scale uniformly.
    fn tx_is_rigid(t: &Transform) -> bool {
        t.b.abs() < 1e-6 && t.c.abs() < 1e-6 && t.a > 1e-6 && (t.a - t.d).abs() < 1e-6
    }

    /// Emits a rounded-rect fill + border through the SDF double-wall
    /// pipeline. Returns false when the appearance or transform isn't
    /// representable (gradient paints, skewed transform); the caller falls
    /// back to a square solid base instead of dropping the rect.
    fn emit_rrect(
        &mut self,
        rect: &ResolvedRect,
        appearance: &riode::foundation::Appearance,
        t: Transform,
        op: f32,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return true;
        }
        if !Self::tx_is_rigid(&t) {
            return false;
        }
        let op = op * appearance.opacity;
        // Fill: solid, or a 2-stop linear gradient (Oklab on the GPU).
        // Radial/N-stop gradients stay unsupported (vello fallback).
        let mut fill = [0.0, 0.0, 0.0, 0.0];
        let mut grad_fill: Option<&riode::foundation::LinearGradient> = None;
        match &appearance.fill {
            Some(Fill::Solid(c)) => {
                let a = (c.a * op).clamp(0.0, 1.0);
                if a > 0.0 {
                    fill = [c.r, c.g, c.b, a];
                }
            }
            Some(Fill::LinearGradient(g)) if g.stops.len() == 2 => {
                grad_fill = Some(g);
            }
            Some(_) => return false,
            None => {}
        }
        // Border: solid only; per-edge widths resolved CPU-side into the
        // outer/inner walls below (the shader never sees alignment).
        // Kept separate from the vertex stroke slot: a gradient fill uses
        // that slot for stop 1 and emits the border as a second vertex.
        let mut border = [0.0, 0.0, 0.0, 0.0];
        // Device-space per-edge border widths (top, right, bottom, left).
        let mut ew = [0.0f32; 4];
        if let Some(s) = &appearance.stroke {
            let Fill::Solid(c) = &s.fill else {
                return false;
            };
            let a = (c.a * op).clamp(0.0, 1.0);
            let w = s.widths();
            let edges = [w.top * t.a, w.right * t.a, w.bottom * t.a, w.left * t.a];
            if a > 0.0 && !edges.iter().all(|e| *e <= 0.0) {
                ew = edges;
                border = [c.r, c.g, c.b, a];
            }
        }
        if fill[3] <= 0.0 && border[3] <= 0.0 && grad_fill.is_none() {
            return true;
        }
        // Device-space layout box + uniformly scaled corner radii.
        let corners = [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y + rect.height),
        ];
        let p0 = t.transform_point(Point::new(corners[0].0, corners[0].1));
        let p1 = t.transform_point(Point::new(corners[1].0, corners[1].1));
        let (dx0, dy0) = (p0.x.min(p1.x), p0.y.min(p1.y));
        let (dx1, dy1) = (p0.x.max(p1.x), p0.y.max(p1.y));
        let rc = appearance.radius.resolve_corners(rect.width, rect.height);
        let s = t.a;
        let radii = [
            rc.top_left * s,
            rc.top_right * s,
            rc.bottom_right * s,
            rc.bottom_left * s,
        ];
        // Alignment resolves into two walls per edge: how far the OUTER
        // wall extends past the layout box (expand), and how far the
        // INNER wall sits inside it (inset). The painted region is the
        // outer box for every alignment, so quad padding is AA-only.
        let align = appearance.stroke.as_ref().map(|st| st.align);
        let (out, ins): ([f32; 4], [f32; 4]) = match align {
            None => ([0.0; 4], [0.0; 4]),
            Some(StrokeAlign::Inside) => ([0.0; 4], ew),
            Some(StrokeAlign::Center) => (ew.map(|w| w * 0.5), ew.map(|w| w * 0.5)),
            Some(StrokeAlign::Outside) => (ew, [0.0; 4]),
        };
        // Invisible border (zero/transparent): fill-only disc (ibox == fbox).
        let (out, ins) = if border[3] <= 0.0 {
            ([0.0; 4], [0.0; 4])
        } else {
            (out, ins)
        };
        let fbox = [
            dx0 - out[3],
            dy0 - out[0],
            (dx1 - dx0) + out[1] + out[3],
            (dy1 - dy0) + out[0] + out[2],
        ];
        let ibox = [
            dx0 + ins[3],
            dy0 + ins[0],
            ((dx1 - dx0) - ins[1] - ins[3]).max(0.0),
            ((dy1 - dy0) - ins[0] - ins[2]).max(0.0),
        ];
        // Inner radius per corner: outer radius minus the two adjacent
        // insets, clamped >= 0 (CSS-like: thick borders eat the curve).
        let iradii = [
            (radii[0] - ins[0].max(ins[3])).max(0.0),
            (radii[1] - ins[0].max(ins[1])).max(0.0),
            (radii[2] - ins[1].max(ins[2])).max(0.0),
            (radii[3] - ins[2].max(ins[3])).max(0.0),
        ];
        let pad = 1.5;
        let (qx0, qy0, qx1, qy1) = (
            fbox[0] - pad,
            fbox[1] - pad,
            fbox[0] + fbox[2] + pad,
            fbox[1] + fbox[3] + pad,
        );
        let quad = [
            (qx0, qy0),
            (qx1, qy0),
            (qx0, qy1),
            (qx1, qy0),
            (qx1, qy1),
            (qx0, qy1),
        ];
        // Resolve a 2-stop linear gradient into the reserved slot: unit
        // direction (rigid transforms keep layout-space direction) and
        // the stop points projected onto that axis from the outer-box
        // origin (the shader's t reference). Colors ride Oklab + alpha in
        // fill/stroke. Degenerate axis/stops collapse to solid stop-1.
        let mut grad = [0.0f32; 4];
        let mut grad_ok = false;
        let mut stroke = border;
        if let Some(g) = grad_fill {
            let (dx, dy) = (g.end.x - g.start.x, g.end.y - g.start.y);
            let len = (dx * dx + dy * dy).sqrt();
            let mut resolved = false;
            if len > 1e-4 {
                let dir = [dx / len, dy / len];
                let at = |u: f32| {
                    t.transform_point(Point::new(
                        g.start.x + dx * u,
                        g.start.y + dy * u,
                    ))
                };
                let es = at(g.stops[0].position);
                let ee = at(g.stops[1].position);
                let (ox, oy) = (fbox[0], fbox[1]);
                let stop0_t = (es.x - ox) * dir[0] + (es.y - oy) * dir[1];
                let stop1_t = (ee.x - ox) * dir[0] + (ee.y - oy) * dir[1];
                if (stop1_t - stop0_t).abs() > 1e-4 {
                    let c0 = g.stops[0].color;
                    let c1 = g.stops[1].color;
                    let l0 = srgb_to_oklab([c0.r, c0.g, c0.b]);
                    let l1 = srgb_to_oklab([c1.r, c1.g, c1.b]);
                    fill = [l0[0], l0[1], l0[2], (c0.a * op).clamp(0.0, 1.0)];
                    stroke = [l1[0], l1[1], l1[2], (c1.a * op).clamp(0.0, 1.0)];
                    grad = [dir[0], dir[1], stop0_t, stop1_t];
                    grad_ok = true;
                    resolved = true;
                }
            }
            if !resolved {
                // Zero-length axis or collapsed stops: solid stop-1 (CSS).
                let c1 = g.stops[1].color;
                let a = (c1.a * op).clamp(0.0, 1.0);
                fill = [c1.r, c1.g, c1.b, a];
                stroke = [0.0; 4];
            }
        }
        // Gradient fill + border: the gradient vertex fills the whole
        // outer box; the border rides a second vertex painted over it.
        // Solid fill keeps the single double-wall vertex below.
        let (gbox, gradii) = if grad_ok {
            (fbox, radii)
        } else {
            (ibox, iradii)
        };
        for (px, py) in quad {
            self.rrect_verts.push(RRectVertex {
                pos: [px, py],
                fbox,
                radii,
                ibox: gbox,
                iradii: gradii,
                fill,
                stroke,
                grad,
            });
        }
        if grad_ok && border[3] > 0.0 {
            for (px, py) in quad {
                self.rrect_verts.push(RRectVertex {
                    pos: [px, py],
                    fbox,
                    radii,
                    ibox,
                    iradii,
                    fill: [0.0, 0.0, 0.0, 0.0],
                    stroke: border,
                    grad: [0.0; 4],
                });
            }
            self.push_rrect_run(12);
        } else {
            self.push_rrect_run(6);
        }
        true
    }

    /// Emits one quad run per shadow layer through the shadow SDF
    /// pipeline. Layers arrive extent-descending (tighter over softer),
    /// the exact order the vello and tiny-skia renderers use, with the
    /// per-layer alpha (light intensity * color.a * layer opacity *
    /// node opacity) baked into the vertex. Returns false on a skewed
    /// transform; the caller drops the shadow with a warning.
    fn emit_shadow(
        &mut self,
        rect: &ResolvedRect,
        radius: Radius,
        shadow: &Shadow,
        light: &LightEnvironment,
        t: Transform,
        op: f32,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return true;
        }
        if shadow.layers().is_empty() || light.intensity <= 0.0 {
            return true;
        }
        if matches!(shadow.shape, ShadowShape::Alpha) {
            warn_once("ShadowShape::Alpha rasterized as Box on wgpu");
        }
        if matches!(shadow.blend, riode::foundation::BlendMode::Multiply) {
            warn_multiply_once();
        }
        if !Self::tx_is_rigid(&t) {
            return false;
        }
        // Device-space layout box (same corner transform as emit_rrect;
        // rigid tx keeps the silhouette axis-aligned).
        let p0 = t.transform_point(Point::new(rect.x, rect.y));
        let p1 = t.transform_point(Point::new(rect.x + rect.width, rect.y + rect.height));
        let (dx0, dy0) = (p0.x.min(p1.x), p0.y.min(p1.y));
        let (dx1, dy1) = (p0.x.max(p1.x), p0.y.max(p1.y));
        let rc = radius.resolve_corners(rect.width, rect.height);
        let s = t.a;
        let radii = [
            rc.top_left * s,
            rc.top_right * s,
            rc.bottom_right * s,
            rc.bottom_left * s,
        ];
        let spread_d = shadow.spread * s;
        // Largest extent first so tighter layers paint over softer ones
        // (identical order to the reference engines).
        let mut order: Vec<usize> = (0..shadow.layers().len()).collect();
        order.sort_by(|&a, &b| {
            shadow.layers()[b]
                .extent
                .partial_cmp(&shadow.layers()[a]
                    .extent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut count = 0u32;
        for i in order {
            let layer = &shadow.layers()[i];
            let a = (light.intensity * shadow.color.a * layer.opacity * op).clamp(0.0, 1.0);
            if a <= 0.0 || layer.extent <= 0.0 {
                continue;
            }
            let (ox, oy) = shadow.layer_offset(light, layer);
            // Spread expands the occluder before the blur; the layer
            // offset translates it (layout px scaled to device px).
            let (bx, by) = (dx0 - spread_d + ox * s, dy0 - spread_d + oy * s);
            let (bw, bh) = (
                (dx1 - dx0) + 2.0 * spread_d,
                (dy1 - dy0) + 2.0 * spread_d,
            );
            if bw <= 0.0 || bh <= 0.0 {
                continue;
            }
            // Core softness is already CSS blur/2 = sigma (web_like).
            let sigma = (layer.softness * s).max(0.0);
            // Blur reach from the already spread-expanded box: the
            // extent/kernel term of the shared layer_pad (the spread
            // term would double-count here), +1.5px AA margin (rrect
            // parity). layer_pad's 3*softness >= 2*sigma always covers
            // the shader's +-2sigma support.
            let pad = layer.extent.max(3.0 * layer.softness.max(0.5)) * s + 1.5;
            let color = [shadow.color.r, shadow.color.g, shadow.color.b, a];
            let params = [sigma, 0.0, 0.0, 0.0];
            let fbox = [bx, by, bw, bh];
            let quad = [
                (bx - pad, by - pad),
                (bx + bw + pad, by - pad),
                (bx - pad, by + bh + pad),
                (bx + bw + pad, by - pad),
                (bx + bw + pad, by + bh + pad),
                (bx - pad, by + bh + pad),
            ];
            for (px, py) in quad {
                self.shadow_verts.push(ShadowVertex {
                    pos: [px, py],
                    fbox,
                    radii,
                    color,
                    params,
                });
            }
            count += 6;
        }
        if count > 0 {
            self.push_shadow_run(count);
        }
        true
    }

    /// Emits one textured quad for an already-resolved GPU image. Corners
    /// ride the current affine (any transform is fine: the UVs stay
    /// 0..1 over the transformed quad, so bilinear sampling remains
    /// correct under skew/rotation); opacity bakes into `color.a`.
    fn emit_image_quad(
        &mut self,
        rect: &ResolvedRect,
        gpu: Arc<ImageGpu>,
        t: Transform,
        op: f32,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || gpu.w == 0 || gpu.h == 0 {
            return;
        }
        let corners = [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x, rect.y + rect.height),
            (rect.x + rect.width, rect.y),
            (rect.x + rect.width, rect.y + rect.height),
            (rect.x, rect.y + rect.height),
        ];
        let uvs = [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];
        let start = self.image_verts.len() as u32;
        for ((px, py), uv) in corners.into_iter().zip(uvs) {
            let p = t.transform_point(Point::new(px, py));
            self.image_verts.push(ImageVertex {
                pos: [p.x, p.y],
                uv,
                color: [1.0, 1.0, 1.0, op],
            });
        }
        let bind = self.image_binds.len() as u32;
        self.image_binds.push(gpu);
        self.ops.push(Op::Images {
            start,
            count: 6,
            scissor: self.clip.current(),
            bind,
        });
    }

    fn push_glyph_run(&mut self, count: u32, linear: bool) {
        let sc = self.clip.current();
        let fresh = std::mem::replace(&mut self.barrier, false);
        let extend = match self.ops.last_mut() {
            Some(Op::Glyphs {
                count: c,
                scissor: s,
                linear: l,
                ..
            }) if *s == sc && *l == linear && !fresh => Some(c),
            _ => None,
        };
        match extend {
            Some(c) => *c += count,
            None => self.ops.push(Op::Glyphs {
                start: self.glyph_verts.len() as u32 - count,
                count,
                scissor: sc,
                linear,
            }),
        }
    }

    /// Repaints the damaged area from base color (opaque base quads).
    ///
    /// Called once per partial frame, before any chunk content, so chunk
    /// snapshots never include these verts. Damage is layout-space;
    /// `scale` converts to the device px the vertex buffer carries.
    /// Alpha rides verbatim (usually 1.0); the replace pipeline writes
    /// the quad color over stale pixels with blending disabled.
    pub fn emit_damage_base(
        &mut self,
        damage: &riode::foundation::DamageRegion,
        scale: f32,
        base: Color,
    ) {
        let c = [base.r, base.g, base.b, base.a];
        for r in damage.rects() {
            let (x0, y0) = (r.x * scale, r.y * scale);
            let (x1, y1) = ((r.x + r.width) * scale, (r.y + r.height) * scale);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            for (px, py) in [(x0, y0), (x1, y0), (x0, y1), (x1, y0), (x1, y1), (x0, y1)] {
                self.rect_verts.push(RectVertex {
                    pos: [px, py],
                    color: c,
                });
            }
            let extend = match self.ops.last_mut() {
                Some(Op::BaseQuads { count, .. }) => Some(count),
                _ => None,
            };
            match extend {
                Some(n) => *n += 6,
                None => self.ops.push(Op::BaseQuads {
                    start: self.rect_verts.len() as u32 - 6,
                    count: 6,
                }),
            }
        }
    }

    fn emit_solid(&mut self, rect: &ResolvedRect, color: Color, t: Transform, op: f32) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let a = (color.a * op).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let c = [color.r, color.g, color.b, a];
        // Stack array, not a collected Vec: this runs per rect (thousands
        // per frame) and the heap trip dwarfs the transform math (N3).
        let p = [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x, rect.y + rect.height),
            (rect.x + rect.width, rect.y + rect.height),
        ]
        .map(|(x, y)| {
            let pt = t.transform_point(Point::new(x, y));
            [pt.x, pt.y]
        });
        for i in [0usize, 1, 2, 1, 3, 2] {
            self.rect_verts.push(RectVertex {
                pos: p[i],
                color: c,
            });
        }
        self.push_rect_run(6);
    }

    /// Emits one text run.
    #[allow(clippy::too_many_arguments)]
    fn emit_text(
        &mut self,
        origin: &Point,
        layout: &Arc<riode::text::layout::TextLayout>,
        color: Color,
        text_ctx: &TextContext,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
        t: Transform,
        op: f32,
    ) {
        let a = (color.a * op).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let c = [color.r, color.g, color.b, a];
        // Coverage quads keep linear filtering: the 2px replicated atlas
        // gutter guarantees border taps read the owning cell, and linear
        // covers the epsilon between the integer device anchors (below)
        // and texel centers — nearest snaps that epsilon to blocky steps
        // ("barely readable"). Device scale: coverage rasters at size*q
        // (engine scaled slots), metrics below arrive in scaled px and
        // are unscaled to layout units before the root transform
        // re-applies the scale.
        let q = quantize_raster_scale(self.scale);
        let sq16 = (q * 16.0).round() as u16;
        for line in layout.lines.iter() {
            for g in line.glyphs.iter() {
                let pen_x = origin.x + g.point.x + line.align_offset;
                let pen_y = origin.y + g.point.y;
                let placed = t.transform_point(Point::new(pen_x, pen_y));
                // Subpixel policy — asymmetric quanta, shared with the
                // tiny-skia reference: X quarters (validated-smooth,
                // matching the legacy phase bins), Y eighths (1/16px
                // residue, so horizontal features never read as doubled
                // rows). The analytic rasterizer integrates coverage at
                // exactly that quantum from one linear outline, so glyph
                // *shape* is identical at every subpixel position — no
                // per-phase hinter re-fit, no per-bin bbox churn. The quad
                // integer-anchors at base+bearing, so stems never resample
                // through fractional filtering either. With baseline
                // Y-snap armed (`B` key, GPUI's 4x1), Y snaps to whole
                // device pixels instead (qy forced 0). Settled runs
                // arrive pre-rounded from core compile, so they take the
                // integer path naturally here.
                let (bx, qx) = quantize_quarter(placed.x);
                let fx = placed.x - bx as f32;
                let (qy, fy) = if riode::text::baseline_snap_y() {
                    let by = placed.y.round() as i32;
                    (0u8, placed.y - by as f32)
                } else {
                    let (by, qy) = quantize_eighth(placed.y);
                    (qy, placed.y - by as f32)
                };
                // Atlas slot distinctness only; the bitmap itself is keyed
                // (key, qx, qy, scale) inside the analytic cache. X fits
                // the legacy low 2 bits, Y the next 3 — the phase packs
                // both natively.
                let phase = (qx as u8) | ((qy as u8) << 2);
                // Single raster: bitmap + bearing together (no TOCTOU).
                // Metrics arrive in scaled px (q==1.0 at unity scale).
                let rastered =
                    text_ctx.raster_glyph_exact_scaled(g.cache_key, qx, qy, self.scale, |bmp| {
                        bmp.and_then(|b| {
                            if b.width == 0 || b.height == 0 {
                                return None;
                            }
                            let entry = atlas.get(g.cache_key, phase, sq16).or_else(|| {
                                atlas.insert_from_coverage(
                                    queue,
                                    g.cache_key,
                                    phase,
                                    sq16,
                                    b.width,
                                    b.height,
                                    b.data,
                                )
                            })?;
                            Some((entry, b.left, b.top))
                        })
                    });
                let Some((e, bl, bt)) = rastered else {
                    continue;
                };
                // Layout-space glyph rect. Placement backs out the *exact*
                // device fractions from the quantized base, collapsing to the
                // reference's integer anchors: device content = (bx + left,
                // by - top), <=1/8px residual — identical math to the CPU
                // blit (tiny-skia `render_text_aligned`). UVs arrive
                // edge-exact from the atlas (2px gutter, no inset).
                let (u0, v0) = (e.u0, e.v0);
                let (u1, v1) = (e.u1, e.v1);
                let (bw, bh) = (e.w as f32 / q, e.h as f32 / q);
                let lx0 = pen_x + (bl as f32 - fx) / q;
                let ly0 = pen_y - (bt as f32 + fy) / q;
                let corners = [
                    (lx0, ly0, u0, v0),
                    (lx0 + bw, ly0, u1, v0),
                    (lx0, ly0 + bh, u0, v1),
                    (lx0 + bw, ly0 + bh, u1, v1),
                ];
                // Stack array, not a collected Vec: per glyph (N3).
                let q: [([f32; 2], [f32; 2]); 4] = corners.map(|(x, y, u, v)| {
                    let pt = t.transform_point(Point::new(x, y));
                    ([pt.x, pt.y], [u, v])
                });
                for i in [0usize, 1, 2, 1, 3, 2] {
                    self.glyph_verts.push(GlyphVertex {
                        pos: q[i].0,
                        uv: q[i].1,
                        color: c,
                    });
                }
                self.push_glyph_run(6, true);
            }
        }
    }

    pub fn emit_chunk(
        &mut self,
        base: &Transform,
        commands: &[SceneCommand],
        text_ctx: &TextContext,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        images: &mut crate::image::ImageCache,
        svgs: &mut crate::svg::SvgCache,
    ) {
        let mut sink = WgpuSink {
            batch: self,
            text_ctx,
            atlas,
            queue,
            device,
            images,
            svgs,
        };
        riode::scene::lower_chunk(commands, base, &mut sink);
    }
}

/// Engine-side raster state for one chunk emission: the batch plus the
/// per-frame GPU resources the placed items resolve against.
pub struct WgpuSink<'a> {
    pub batch: &'a mut Batch,
    pub text_ctx: &'a TextContext,
    pub atlas: &'a mut GlyphAtlas,
    pub queue: &'a wgpu::Queue,
    pub device: &'a wgpu::Device,
    pub images: &'a mut crate::image::ImageCache,
    pub svgs: &'a mut crate::svg::SvgCache,
}

impl SceneSink for WgpuSink<'_> {
    fn push_clip(&mut self, clip: &PlacedClip) {
        let t = self.batch.device_tx(&clip.transform);
        self.batch.push_clip_local(&clip.rect, clip.radius, t);
    }

    fn pop_clip(&mut self) {
        self.batch.clip.pop();
    }

    fn rect(&mut self, r: &PlacedRect) {
        let t = self.batch.device_tx(&r.transform);
        let plain_square =
            matches!(r.appearance.radius, Radius::Scalar(v) if v <= 0.0)
                && r.appearance.stroke.is_none();
        match &r.appearance.fill {
            Some(Fill::Solid(c)) => {
                let mut col = *c;
                col.a *= r.appearance.opacity;
                if plain_square {
                    self.batch.emit_solid(&r.rect, col, t, r.opacity);
                } else if !self.batch.emit_rrect(&r.rect, r.appearance, t, r.opacity) {
                    // Skewed transform or gradient stroke:
                    // square solid base keeps layout visible.
                    warn_once("rrect unsupported, square fallback");
                    self.batch.emit_solid(&r.rect, col, t, r.opacity);
                }
            }
            Some(_) => {
                // 2-stop linear gradients ride the SDF pipeline
                // (emit_rrect, Oklab + dither on the GPU);
                // radial/N-stop fall through unsupported.
                if !self.batch.emit_rrect(&r.rect, r.appearance, t, r.opacity) {
                    warn_once("gradient unsupported (radial/N-stop), skipped");
                }
            }
            None => {
                // Stroke-only ring.
                if !plain_square
                    && !self.batch.emit_rrect(&r.rect, r.appearance, t, r.opacity)
                {
                    warn_once("stroke-only rrect unsupported, skipped");
                }
            }
        }
    }

    fn image(&mut self, i: &PlacedImage) {
        let t = self.batch.device_tx(&i.transform);
        if let Some(gpu) = self.images.get_or_insert(self.device, self.queue, i.image) {
            self.batch.emit_image_quad(&i.rect, gpu, t, i.opacity);
        }
    }

    fn svg(&mut self, s: &PlacedSvg) {
        let t = self.batch.device_tx(&s.transform);
        if let Some(gpu) = self.svgs.get_or_insert(
            self.device,
            self.queue,
            self.images,
            s.graphic,
            &s.rect,
            self.batch.scale(),
        ) {
            self.batch.emit_image_quad(&s.rect, gpu, t, s.opacity);
        }
    }

    fn shadow(&mut self, s: &PlacedShadow) {
        let t = self.batch.device_tx(&s.transform);
        if !self
            .batch
            .emit_shadow(&s.rect, s.radius, s.shadow, s.light, t, s.opacity)
        {
            warn_once("shadow unsupported (skewed transform), skipped");
        }
    }

    fn text(&mut self, t: &PlacedText) {
        let tx = self.batch.device_tx(&t.transform);
        self.batch.emit_text(
            &t.origin,
            t.layout,
            t.color,
            self.text_ctx,
            self.atlas,
            self.queue,
            tx,
            t.opacity,
        );
    }
}

impl Default for Batch {
    fn default() -> Self {
        Self::new()
    }
}
