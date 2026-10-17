// Single responsibility: SceneCommand -> batched rect + glyph vertex emission.

use riode::foundation::{
    composed_blur_aspect, composed_blur_sigma, merge_unions, pack_glass_uniforms, Color, Fill,
    GlassFrameUniforms, GlassLayer, GlassShapeUniform, LightEnvironment, Point, Radius,
    ResolvedRect, Shadow, ShadowShape, StrokeAlign, Transform, DEFAULT_LIGHT_ANGLE, GLASS_STANDARD,
};
use riode::scene::command::SceneCommand;
use riode::scene::{
    quantize_quarter, quantize_y, PlacedClip, PlacedImage, PlacedLayer, PlacedRect,
    PlacedShadow, PlacedSvg, PlacedText, SceneSink,
};
use riode::text::{quantize_raster_scale, TextContext};
use std::sync::Arc;

use crate::clip::ClipStack;
use crate::image::ImageGpu;
use crate::pipeline::{
    GlyphVertex, GlassFrameGpu, ImageVertex, RRectVertex, RectVertex, ShadowVertex,
};
use crate::text::GlyphAtlas;

static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WARNED_MULTIPLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WARNED_GLASS_BLEND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// Blend on a glass-carrying layer: blending the whole card needs the
/// lens+content result composited once against the backdrop (separate
/// work) — the blend is ignored here, source-over as before.
fn warn_glass_blend_once() {
    if !WARNED_GLASS_BLEND.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!(
            "riode-wgpu: blend mode on a glass layer ignored (further silent); glass-card blend is separate work"
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
#[derive(Clone)]
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
    /// Opens a filter layer: draw ops after this target the layer's
    /// surface-sized offscreen instead of the parent target. `region` is
    /// the device-space content AABB padded by 3σ (blur reach + the
    /// composite quad) and the glass damage pad; `params` indexes the
    /// batch's `filter_params`; `glass` indexes `glass_params` when the
    /// layer carries a liquid-glass material (the renderer snapshots
    /// the parent under the padded region and runs the optics pass).
    /// A marker op (no draws): it also breaks run merging by variant.
    BeginLayer {
        region: ResolvedRect,
        params: u32,
        glass: Option<u32>,
    },
    /// Closes the innermost filter layer: the renderer runs the blur
    /// passes over the layer's offscreen, then draws the composite quad
    /// `start..start + count` (rect_verts, the region) onto the parent
    /// through `scissor` (ancestor clips; in-layer clips died with the
    /// layer's balanced pops). Alpha-silhouette shadow draws sit first:
    /// quads `shadow_start..shadow_start + 6·shadows.len()`, one blur +
    /// tinted composite per [`ShadowGpuDraw`], painted behind the content
    /// composite in vertex order.
    EndLayer {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
        shadow_start: u32,
        shadows: Vec<ShadowGpuDraw>,
        /// Dedicated LightSpill verts (`spill_start..spill_start +
        /// spill_count`, rect_verts): the below-edge strip with feathered
        /// vertex alpha (horizontal edge fade + vertical falloff). Drawn
        /// with the blurred-card material when the replay is active.
        spill_start: u32,
        spill_count: u32,
    },
}

/// One alpha-silhouette shadow layer, emission-time in device px: the
/// separable blur's sigma, the composite quad's displacement (texel
/// lookup shifts by `-offset`), and the row-major 4×5 tint matrix —
/// constant RGB rows of the shadow color, alpha row scaled by
/// intensity × color alpha × layer opacity × node opacity.
#[derive(Clone)]
pub struct ShadowGpuDraw {
    pub sigma: f32,
    pub offset: [f32; 2],
    pub matrix: [f32; 20],
}

/// One alpha drop shadow stashed on its layer (emission order: BeginLayer
/// → shadow → fill): `DrawShadow` inside the element's own offscreen is a
/// data carrier, painted by `end_layer` from the layer's finished alpha.
#[derive(Clone)]
pub struct AlphaShadow {
    pub shadow: Shadow,
    pub light: LightEnvironment,
    pub opacity: f32,
    pub transform: Transform,
}

/// LightSpill seed for one glass layer: the downward length in device
/// px, or `None` when the style throws no spill. `resolve_rule()[0]` is
/// the length in logical px (the alias maps `reach` × 32 back to it).
fn spill_len(r: &riode::foundation::Refraction, scale: f32) -> Option<f32> {
    if !r.is_active() {
        return None;
    }
    let len = r.resolve_rule()[0].max(0.0) * scale;
    (len > 0.5).then_some(len)
}

/// One open layer, emission-side: the offscreen region + filter-params
/// index (`Some`), or `None` for a plain clip layer, plus the stashed
/// alpha shadow cast from this layer's content at `end_layer`.
struct LayerStackEntry {
    region: Option<(ResolvedRect, u32)>,
    alpha: Option<AlphaShadow>,
    /// LightSpill seed: card-union bounds (x-span + bottom edge, device
    /// px) + spill length (device px) + corner radius (device px).
    /// `end_layer` emits the feathered spill verts from it.
    spill: Option<(ResolvedRect, f32, f32)>,
}

/// Resolved element filter for one layer, emission-time: the chain's
/// largest blur sigma in device px (sigma scales with the root device
/// matrix, unlike the tiny-skia twin which renders unscaled) plus the
/// combined row-major 4×5 CSS color matrix (identity when the chain is
/// color-free). `Batch::filter_params` holds one per filter layer,
/// indexed by `Op::BeginLayer.params`.
///
/// Frame-local: layer chunks are non-rebaseable, so a cache replay can
/// never read another frame's index.
#[derive(Clone, Copy)]
pub struct FilterGpu {
    pub sigma: f32,
    pub matrix: [f32; 20],
    /// Blend mode the layer's content composites at against the parent
    /// snapshot, packed as the wire id (`BlendMode::as_uniform_id`);
    /// `0.0` = `Normal` = plain source-over through the regular
    /// composite pipeline. The renderer switches to the blend pipeline
    /// when it is non-zero (and the layer carries no glass).
    pub blend: f32,
}

impl FilterGpu {
    /// Row-major 4×5 identity: RGB pass-through, alpha preserved, no offset.
    pub const IDENTITY: [f32; 20] = [
        1.0, 0.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
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
    /// Resolved filters for the frame's filter layers, indexed by
    /// `Op::BeginLayer.params`.
    pub filter_params: Vec<FilterGpu>,
    /// Packed glass frame uniforms for the frame's glass layers, indexed
    /// by `Op::BeginLayer.glass`. Packed at emission (device px, device
    /// scale folded in); frame-local for the same reason as
    /// `filter_params`.
    pub glass_params: Vec<GlassFrameGpu>,
    /// Emission-side layer stack, one entry per open layer: an offscreen
    /// region + params index for a filter/alpha-shadow layer, `None` for
    /// a plain clip layer (the trait-default degradation, no offscreen).
    /// Balanced by `WgpuSink::end_layer`; a non-empty stack at `reset`
    /// means an unbalanced command stream.
    layer_stack: Vec<LayerStackEntry>,
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
            filter_params: Vec::new(),
            glass_params: Vec::new(),
            layer_stack: Vec::new(),
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
        self.filter_params.clear();
        self.glass_params.clear();
        if !self.layer_stack.is_empty() {
            tracing::warn!(
                "riode-wgpu: {} unclosed filter layer(s) at frame end (unbalanced BeginLayer/EndLayer)",
                self.layer_stack.len()
            );
            self.layer_stack.clear();
        }
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
        // Original (un-offset, un-spread) box: the interior mask for
        // inset layers; zeroed for drop layers (the shader never reads it).
        let obox = if shadow.inset {
            [dx0, dy0, dx1 - dx0, dy1 - dy0]
        } else {
            [0.0; 4]
        };
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
            let params = [sigma, if shadow.inset { 1.0 } else { 0.0 }, 0.0, 0.0];
            let fbox = [bx, by, bw, bh];
            // Inset: the visible region is the element interior only — the
            // quad hugs the original box (+1.5px AA) instead of the
            // offset+blur reach. The shader clips to obox anyway; this
            // just avoids rasterizing outside.
            let (qx0, qy0, qx1, qy1) = if shadow.inset {
                (
                    obox[0] - 1.5,
                    obox[1] - 1.5,
                    obox[0] + obox[2] + 1.5,
                    obox[1] + obox[3] + 1.5,
                )
            } else {
                (bx - pad, by - pad, bx + bw + pad, by + bh + pad)
            };
            let quad = [
                (qx0, qy0),
                (qx1, qy0),
                (qx0, qy1),
                (qx1, qy0),
                (qx1, qy1),
                (qx0, qy1),
            ];
            for (px, py) in quad {
                self.shadow_verts.push(ShadowVertex {
                    pos: [px, py],
                    fbox,
                    radii,
                    color,
                    params,
                    obox,
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
                // matching the legacy phase bins), Y halves (1/4px
                // residue). The analytic rasterizer integrates coverage at
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
                    let (by, qy) = quantize_y(placed.y);
                    (qy, placed.y - by as f32)
                };
                // Atlas slot distinctness only; the bitmap itself is keyed
                // (key, qx, qy, scale) inside the analytic cache. X fits
                // the legacy low 2 bits, Y the next 1 — the phase packs
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

    fn begin_layer(&mut self, layer: &PlacedLayer) {
        // A plain layer is a clip — unless a filter, glass material,
        // alpha-shadow pad, or a non-`Normal` blend mode demands the
        // offscreen (the blend composites against a parent snapshot).
        if layer.filter.is_none()
            && layer.shadow_pad <= 0.0
            && layer.glass.is_none()
            && layer.blend.is_normal()
        {
            // Plain isolation layer (pre-filter behavior): the trait
            // default — a clip, so unfiltered groups keep today's
            // semantics. The stack still needs the entry so end_layer
            // knows to pop this clip instead of compositing.
            self.push_clip(&PlacedClip {
                rect: layer.rect,
                radius: Radius::ZERO,
                transform: layer.transform,
            });
            self.batch.layer_stack.push(LayerStackEntry {
                region: None,
                alpha: None,
                spill: None,
            });
            return;
        };
        // Device-space AABB of the layer rect under its placed transform
        // (root device scale composed in — same corner walk as
        // `push_clip_local`). The offscreen is surface-sized, so unlike
        // the tiny-skia twin there is no tile clamp: scissors and the
        // composite quad are clamped at draw time.
        let t = self.batch.device_tx(&layer.transform);
        let mut x0 = f32::INFINITY;
        let mut y0 = f32::INFINITY;
        let mut x1 = f32::NEG_INFINITY;
        let mut y1 = f32::NEG_INFINITY;
        for (px, py) in [
            (layer.rect.x, layer.rect.y),
            (layer.rect.x + layer.rect.width, layer.rect.y),
            (layer.rect.x, layer.rect.y + layer.rect.height),
            (
                layer.rect.x + layer.rect.width,
                layer.rect.y + layer.rect.height,
            ),
        ] {
            let p = t.transform_point(Point::new(px, py));
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        // Exact device rect of the layer box (for the glass SDF) —
        // taken before the pad expands the AABB below.
        let rect_dev = (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0));
        // Gaussian support: 3σ past the content AABB so blur tails stay
        // complete, in device px (σ scales with the root matrix; the
        // tiny-skia engine renders unscaled, so its σ is already
        // layout=device), the glass folding/blur reach, plus the
        // alpha-shadow pad (offset + blur reach of the silhouette cast
        // this layer exists for).
        let sigma = layer
            .filter
            .map(|f| f.max_blur_sigma())
            .unwrap_or(0.0)
            * self.batch.scale();
        let mut pad = 3.0 * sigma + layer.shadow_pad * self.batch.scale();
        if let Some(g) = layer.glass {
            pad += g.device_pad(self.batch.scale());
        }
        let region = ResolvedRect::new(
            x0 - pad,
            y0 - pad,
            (x1 - x0) + 2.0 * pad,
            (y1 - y0) + 2.0 * pad,
        );
        let params = self.batch.filter_params.len() as u32;
        // Glass + blend: the card composites source-over (Phase 3 owns
        // blending the whole card against the backdrop); fold the mode
        // to `Normal` so the EndLayer path agrees with the snapshot it
        // actually took.
        if layer.glass.is_some() && !layer.blend.is_normal() {
            warn_glass_blend_once();
        }
        self.batch.filter_params.push(FilterGpu {
            sigma,
            matrix: layer
                .filter
                .and_then(|f| f.color_matrix())
                .unwrap_or(FilterGpu::IDENTITY),
            blend: if layer.glass.is_some() {
                0.0
            } else {
                layer.blend.as_uniform_id()
            },
        });
        let (glass, spill_seed) = match layer.glass {
            None => (None, None),
            Some(g) => {
                let (idx, spill) = match g {
            GlassLayer::Single(g, b, s, gl, r) => {
                let sc = self.batch.scale();
                let k = GLASS_STANDARD;
                let v = g.effective(&b);
                // SDF rect: the device AABB before the pad expanded it.
                let rect = ResolvedRect::new(rect_dev.0, rect_dev.1, rect_dev.2, rect_dev.3);
                // One radius per shape (the reference's uniform carries a
                // single value): per-corner riode radii collapse to the
                // smallest, capped at half the shorter side — the same rule
                // `merge_unions` keeps. A pill (`Radius::Max`) resolves equal
                // corners, so it is unaffected.
                let c = g.radius.resolve_corners(layer.rect.width, layer.rect.height);
                let radius = c
                    .top_left
                    .min(c.top_right)
                    .min(c.bottom_right)
                    .min(c.bottom_left)
                    * sc;
                let radius = radius.min(rect.width.min(rect.height) / 2.0);
                // Core blur the engine pre-applies to the snapshot, composed
                // from the shape's half shorter side (logical px in and out);
                // the shader spends `uBlur` only on what this one misses.
                let half_min = rect.width.min(rect.height) / 2.0 / sc;
                let sigma = composed_blur_sigma(v, half_min);
                let aspect = composed_blur_aspect(v, rect.width / sc, rect.height / sc);
                let mut shape = GlassShapeUniform::new(rect, radius, g.hidden);
                shape.tint = b.tint;
                shape.corner_exponent = g.corner_exponent;
                shape.blur_sigma = sigma;
                shape.blur_aspect = aspect;
                shape.strength = g.strength;
                shape.band = g.band;
                shape.rim = g.rim;
                shape.frost_sigma = b.blur;
                shape.wash = b.wash;
                shape.saturation = b.saturation;
                shape.rim_intensity = g.rim_intensity;
                shape.rim_mix = g.rim_mix;
                shape.blur_aspect_power = g.blur_aspect_power;
                shape.lens_edge_decay = g.lens_edge_decay;
                shape.tone = b.tone;
                shape.tone_curve = b.tone_curve;
                shape.lens_decay = g.lens_decay;
                shape.lens_size_ref = g.lens_size_ref;
                shape.lens_edge = g.lens_edge;
                shape.normal_radius_scale = g.normal_radius_scale;
                shape.rim_width = g.rim_width;
                shape.rim_mix_width = g.rim_mix_width;
                shape.rim_mix_cut = g.rim_mix_cut;
                shape.rim_mix_floor = g.rim_mix_floor;
                shape.tint_strength = b.tint_strength;
                shape.ambient_mix = b.ambient_mix;
                shape.post_blur_share = g.post_blur_share;
                shape.frost_wide_sigma = g.frost_wide_sigma;
                shape.frost_wide_mix_edge = g.frost_wide_mix_edge;
                shape.frost_wide_mix_centre = g.frost_wide_mix_centre;
                shape.frost_wide_size_ref = g.frost_wide_size_ref;
                shape.frost_wide_size_drop = g.frost_wide_size_drop;
                shape.blur_size_ref = b.blur_size_ref;
                shape.fill_size_ref = b.fill_size_ref;
                shape.fill_size_drop = b.fill_size_drop;
                shape.glow_strength = g.glow_strength;
                shape.lens_strength = g.lens_strength;
                shape.press_lift = g.press_lift;
                shape.dim = b.dim;
                shape.post_jacobian_max = g.post_jacobian_max;
                shape.ambient_reach = b.ambient_reach;
                let frame = GlassFrameUniforms {
                    shapes: std::slice::from_ref(&shape),
                    device_pixel_ratio: sc,
                    light_angle: DEFAULT_LIGHT_ANGLE,
                    // A lone shape has no smin partner; the frame smoothing
                    // only matters for multi-shape groups (Phase 5).
                    smoothing: 0.0,
                    constants: &k,
                    high_contrast: false,
                    opaque_color: None,
                    touch: None,
                    glow: 0.0,
                    blur_sigma: sigma,
                    blur_aspect: aspect,
                    spectral: *s,
                    voice_glow: *gl,
                    refraction: *r,
                };
                let packed = pack_glass_uniforms(&frame);
                let idx = self.batch.glass_params.len() as u32;
                self.batch.glass_params.push(GlassFrameGpu::from_pack(
                    [sigma * aspect * sc, sigma / aspect * sc, 0.0, 0.0],
                    &packed,
                ));
                // LightSpill seed: x-span + rounded bottom edge of the
                // card rect with the downward length and corner radius,
                // so `end_layer` can emit the feathered spill verts.
                let spill = spill_len(r, sc).map(|len| {
                    (
                        ResolvedRect::new(
                            rect.x,
                            rect.y + rect.height,
                            rect.width.max(0.0),
                            len,
                        ),
                        len,
                        radius,
                    )
                });
                (idx, spill)
            }
            GlassLayer::Group(group) => {
                let sc = self.batch.scale();
                let k = GLASS_STANDARD;
                // Group-layer-local logical px to device px: the same
                // corner walk as the layer box above, per member (and one
                // for the shared clip).
                let dev_box = |r: ResolvedRect| {
                    let mut x0 = f32::INFINITY;
                    let mut y0 = f32::INFINITY;
                    let mut x1 = f32::NEG_INFINITY;
                    let mut y1 = f32::NEG_INFINITY;
                    for (px, py) in [
                        (r.x, r.y),
                        (r.x + r.width, r.y),
                        (r.x, r.y + r.height),
                        (r.x + r.width, r.y + r.height),
                    ] {
                        let p = t.transform_point(Point::new(px, py));
                        x0 = x0.min(p.x);
                        y0 = y0.min(p.y);
                        x1 = x1.max(p.x);
                        y1 = y1.max(p.y);
                    }
                    ResolvedRect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
                };
                let clip_dev = group.clip.map(|c| dev_box(c));
                let mut shapes = Vec::with_capacity(group.members.len().min(24));
                for m in &group.members {
                    let rect = dev_box(m.rect);
                    let v = m.glass.effective(&m.backdrop);
                    let radius = (m.radius() * m.radius_scale * sc)
                        .min(rect.width.min(rect.height) / 2.0);
                    let half_min = rect.width.min(rect.height) / 2.0 / sc;
                    let sigma = composed_blur_sigma(v, half_min);
                    let aspect = composed_blur_aspect(v, rect.width / sc, rect.height / sc);
                    let mut shape = GlassShapeUniform::new(rect, radius, m.glass.hidden);
                    shape.tint = m.backdrop.tint;
                    shape.corner_exponent = m.glass.corner_exponent;
                    shape.union_id = m.glass.union_id;
                    shape.group = 0;
                    shape.smoothing = Some(group.smoothing * sc);
                    shape.opacity = group.opacity;
                    shape.clip = clip_dev;
                    shape.light = m.light;
                    shape.blur_sigma = sigma;
                    shape.blur_aspect = aspect;
                    shape.strength = m.glass.strength;
                    shape.lens_strength = m.glass.lens_strength;
                    shape.band = m.glass.band;
                    shape.rim = m.glass.rim;
                    shape.frost_sigma = m.backdrop.blur;
                    shape.wash = m.backdrop.wash;
                    shape.saturation = m.backdrop.saturation;
                    shape.rim_intensity = m.glass.rim_intensity;
                    shape.rim_mix = m.glass.rim_mix;
                    shape.blur_aspect_power = m.glass.blur_aspect_power;
                    shape.lens_edge_decay = m.glass.lens_edge_decay;
                    shape.tone = m.backdrop.tone;
                    shape.tone_curve = m.backdrop.tone_curve;
                    shape.lens_decay = m.glass.lens_decay;
                    shape.lens_size_ref = m.glass.lens_size_ref;
                    shape.lens_edge = m.glass.lens_edge;
                    shape.normal_radius_scale = m.glass.normal_radius_scale;
                    shape.rim_width = m.glass.rim_width;
                    shape.rim_mix_width = m.glass.rim_mix_width;
                    shape.rim_mix_cut = m.glass.rim_mix_cut;
                    shape.rim_mix_floor = m.glass.rim_mix_floor;
                    shape.tint_strength = m.backdrop.tint_strength;
                    shape.ambient_mix = m.backdrop.ambient_mix;
                    shape.post_blur_share = m.glass.post_blur_share;
                    shape.frost_wide_sigma = m.glass.frost_wide_sigma;
                    shape.frost_wide_mix_edge = m.glass.frost_wide_mix_edge;
                    shape.frost_wide_mix_centre = m.glass.frost_wide_mix_centre;
                    shape.frost_wide_size_ref = m.glass.frost_wide_size_ref;
                    shape.frost_wide_size_drop = m.glass.frost_wide_size_drop;
                    shape.blur_size_ref = m.backdrop.blur_size_ref;
                    shape.fill_size_ref = m.backdrop.fill_size_ref;
                    shape.fill_size_drop = m.backdrop.fill_size_drop;
                    shape.glow_strength = m.glass.glow_strength;
                    shape.press_lift = m.glass.press_lift;
                    shape.dim = m.backdrop.dim;
                    shape.post_jacobian_max = m.glass.post_jacobian_max;
                    shape.ambient_reach = m.backdrop.ambient_reach;
                    shapes.push(shape);
                }
                // Union merge (Phase 5.3): same-`union_id` members draw as
                // one surface before the pack.
                let shapes = merge_unions(&shapes);
                // One blur for the whole pass at the smallest drawn sigma
                // (the reference's rule); members wanting more get the
                // rest as extra frost taps (`uBlur`).
                let (frame_sigma, frame_aspect) = shapes
                    .iter()
                    .map(|s| (s.blur_sigma, s.blur_aspect))
                    .min_by(|a, b| {
                        a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or((0.0, 1.0));
                // Frame touch: the pressed member's touch in device
                // px, with its glow (`uTouch`).
                let (touch, glow) = group
                    .press_touch
                    .map(|(local, glow)| {
                        let p = t.transform_point(Point::new(local.x, local.y));
                        ([p.x, p.y], glow)
                    })
                    .unwrap_or(([0.0, 0.0], 0.0));
                let frame = GlassFrameUniforms {
                    shapes: &shapes,
                    device_pixel_ratio: sc,
                    light_angle: DEFAULT_LIGHT_ANGLE,
                    smoothing: 0.0,
                    constants: &k,
                    high_contrast: false,
                    opaque_color: None,
                    touch: if group.press_touch.is_some() {
                        Some(touch)
                    } else {
                        None
                    },
                    glow,
                    blur_sigma: frame_sigma,
                    blur_aspect: frame_aspect,
                    spectral: group.spectral,
                    voice_glow: group.glow,
                    refraction: group.refraction,
                };
                let packed = pack_glass_uniforms(&frame);
                let idx = self.batch.glass_params.len() as u32;
                self.batch.glass_params.push(GlassFrameGpu::from_pack(
                    [
                        frame_sigma * frame_aspect * sc,
                        frame_sigma / frame_aspect * sc,
                        0.0,
                        0.0,
                    ],
                    &packed,
                ));
                // LightSpill seed: x-span union + rounded bottom edge of
                // the merged member rects, with the downward length and
                // the smallest corner radius.
                let mut ux0 = f32::INFINITY;
                let mut uy1 = f32::NEG_INFINITY;
                let mut ux1 = f32::NEG_INFINITY;
                let mut ur = f32::INFINITY;
                for s in &shapes {
                    ux0 = ux0.min(s.rect.x);
                    uy1 = uy1.max(s.rect.y + s.rect.height);
                    ux1 = ux1.max(s.rect.x + s.rect.width);
                    ur = ur.min(s.radius);
                }
                let spill = spill_len(&group.refraction, sc).and_then(|len| {
                    (ux0.is_finite() && ux1 > ux0).then(|| {
                        (ResolvedRect::new(ux0, uy1, (ux1 - ux0).max(0.0), len), len, ur.max(0.0))
                    })
                });
                (idx, spill)
                } // end Group arm
            }; // end match g
                (Some(idx), spill)
            }
        };
        self.batch.layer_stack.push(LayerStackEntry {
            region: Some((region, params)),
            alpha: None,
            spill: spill_seed,
        });
        self.batch
            .ops
            .push(Op::BeginLayer { region, params, glass });
    }

    fn end_layer(&mut self) {
        match self.batch.layer_stack.pop() {
            Some(LayerStackEntry {
                region: Some((region, _params)),
                alpha,
                spill,
            }) => {
                // Alpha-silhouette shadow quads first (painted behind the
                // content composite below): one 6-vert quad per shadow
                // layer at the region displaced by the layer's device
                // offset, tinted through a constant-color matrix at draw
                // time (see renderer EndLayer).
                let shadow_start = self.batch.rect_verts.len() as u32;
                let mut shadows: Vec<ShadowGpuDraw> = Vec::new();
                if let Some(a) = alpha {
                    if a.shadow.spread != 0.0 {
                        warn_once("alpha shadow: spread ignored (CSS drop-shadow has none)");
                    }
                    if matches!(
                        a.shadow.blend,
                        riode::foundation::BlendMode::Multiply
                    ) {
                        warn_multiply_once();
                    }
                    let t = self.batch.device_tx(&a.transform);
                    let sc = self.batch.scale();
                    let mut order: Vec<usize> =
                        (0..a.shadow.layers().len()).collect();
                    order.sort_by(|&x, &y| {
                        a.shadow.layers()[y]
                            .extent
                            .partial_cmp(&a.shadow.layers()[x].extent)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let c = &a.shadow.color;
                    for i in order {
                        let layer = &a.shadow.layers()[i];
                        let op = (a.light.intensity * c.a * layer.opacity * a.opacity)
                            .clamp(0.0, 1.0);
                        if op <= 0.0 {
                            continue;
                        }
                        let (ox, oy) = a.shadow.layer_offset(&a.light, layer);
                        let (dx, dy) = (t.a * ox + t.c * oy, t.b * ox + t.d * oy);
                        let sigma = (layer.softness * sc).max(0.0);
                        shadows.push(ShadowGpuDraw {
                            sigma,
                            offset: [dx, dy],
                            matrix: [
                                0.0, 0.0, 0.0, 0.0, c.r, //
                                0.0, 0.0, 0.0, 0.0, c.g, //
                                0.0, 0.0, 0.0, 0.0, c.b, //
                                0.0, 0.0, 0.0, op, 0.0,
                            ],
                        });
                        let (qx0, qy0) = (region.x + dx, region.y + dy);
                        let (qx1, qy1) =
                            (qx0 + region.width, qy0 + region.height);
                        for (px, py) in [
                            (qx0, qy0),
                            (qx1, qy0),
                            (qx0, qy1),
                            (qx1, qy0),
                            (qx1, qy1),
                            (qx0, qy1),
                        ] {
                            self.batch.rect_verts.push(RectVertex {
                                pos: [px, py],
                                color: [1.0; 4],
                            });
                        }
                    }
                }
                // Composite quad over the filtered offscreen: six rect
                // verts at the region (device px). The composite shader
                // derives texel coords from position — no UVs needed.
                let start = self.batch.rect_verts.len() as u32;
                let tint = [1.0; 4];
                let (x0, y0) = (region.x, region.y);
                let (x1, y1) = (region.x + region.width, region.y + region.height);
                for (px, py) in [
                    (x0, y0),
                    (x1, y0),
                    (x0, y1),
                    (x1, y0),
                    (x1, y1),
                    (x0, y1),
                ] {
                    self.batch.rect_verts.push(RectVertex {
                        pos: [px, py],
                        color: tint,
                    });
                }
                // LightSpill verts: a strip of side-by-side quads below
                // the card's bottom edge, its top following the rounded
                // corners in 8 steps per side. Vertex alpha carries the
                // shaping the scissor cannot: top row opaque → bottom row
                // transparent (vertical falloff), outer side columns
                // transparent → inner opaque (horizontal feather). The
                // blurred-card material supplies colour + its own blur
                // falloff; the matrix gain keeps it dimmer than the card.
                let (spill_start, spill_count) = match spill {
                    Some((bounds, len, radius)) => {
                        let s = self.batch.rect_verts.len() as u32;
                        let (sx0, syb) = (bounds.x, bounds.y);
                        let sy1 = bounds.y + len;
                        let w = bounds.width.max(0.0);
                        if w <= 0.0 {
                            (s, 0)
                        } else {
                            let r = radius.clamp(0.0, w * 0.5);
                            // Horizontal feather: 10% of the width on each
                            // side — the box edges never paint a hard line.
                            let f = (w * 0.10).clamp(0.0, 40.0);
                            // Bottom-edge height at x along the rounded
                            // corners (circle arcs, straight between).
                            let edge_y = |x: f32| -> f32 {
                                if r <= 0.0 {
                                    return syb;
                                }
                                if x < sx0 + r {
                                    let dx = (sx0 + r) - x;
                                    syb - r + (r * r - dx * dx).max(0.0).sqrt()
                                } else if x > sx0 + w - r {
                                    let dx = x - (sx0 + w - r);
                                    syb - r + (r * r - dx * dx).max(0.0).sqrt()
                                } else {
                                    syb
                                }
                            };
                            let fade_x = |x: f32| -> f32 {
                                if f <= 0.0 {
                                    return 1.0;
                                }
                                ((x - sx0) / f).clamp(0.0, 1.0)
                                    * ((sx0 + w - x) / f).clamp(0.0, 1.0)
                            };
                            // Column boundaries: 8 steps up each corner
                            // arc, one straight span between.
                            let k = 8u32;
                            let mut xs = Vec::with_capacity((2 * k + 2) as usize);
                            xs.push(sx0);
                            for i in 1..=k {
                                xs.push(sx0 + r * (i as f32) / (k as f32));
                            }
                            for i in 0..=k {
                                xs.push(sx0 + w - r + r * (i as f32) / (k as f32));
                            }
                            for j in 0..xs.len() - 1 {
                                let (xa, xb) = (xs[j], xs[j + 1]);
                                if xb <= xa {
                                    continue;
                                }
                                let (ya, yb) = (edge_y(xa), edge_y(xb));
                                let (aa, ab) = (fade_x(xa), fade_x(xb));
                                for (px, py, a) in [
                                    (xa, ya, aa),
                                    (xb, yb, ab),
                                    (xa, sy1, 0.0),
                                    (xb, yb, ab),
                                    (xb, sy1, 0.0),
                                    (xa, sy1, 0.0),
                                ] {
                                    self.batch.rect_verts.push(RectVertex {
                                        pos: [px, py],
                                        color: [1.0, 1.0, 1.0, a],
                                    });
                                }
                            }
                            let n =
                                (self.batch.rect_verts.len() as u32).saturating_sub(s);
                            (s, n)
                        }
                    }
                    None => (0, 0),
                };
                // In-layer clips are balanced pops inside the layer, so
                // `current()` is exactly the ancestor clips: what must
                // bound the composite on the parent target.
                let scissor = self.batch.clip.current();
                self.batch.ops.push(Op::EndLayer {
                    start,
                    count: 6,
                    scissor,
                    shadow_start,
                    shadows,
                    spill_start,
                    spill_count,
                });
            }
            Some(LayerStackEntry { region: None, .. }) => {
                // Clip layer: pop the clip pushed in begin_layer.
                self.batch.clip.pop();
            }
            None => {
                tracing::warn!(
                    "riode-wgpu: layer stack underflow: EndLayer without a matching BeginLayer"
                );
                self.batch.clip.pop();
            }
        }
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
        // Alpha silhouette inside its own offscreen layer: stash instead
        // of painting — `end_layer` casts it from the finished layer
        // alpha, behind the content composite.
        if matches!(s.shadow.shape, ShadowShape::Alpha) && !s.shadow.inset {
            if let Some(e) = self
                .batch
                .layer_stack
                .last_mut()
                .filter(|e| e.region.is_some())
            {
                e.alpha = Some(AlphaShadow {
                    shadow: s.shadow.clone(),
                    light: *s.light,
                    opacity: s.opacity,
                    transform: s.transform,
                });
                return;
            }
        }
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
