// Single responsibility: SceneCommand -> batched rect + glyph vertex emission.

use riode::foundation::{Color, Fill, Point, Radius, ResolvedRect, StrokeAlign, Transform};
use riode::scene::command::SceneCommand;
use riode::text::{quantize_raster_scale, TextContext};
use std::sync::Arc;

use crate::clip::ClipStack;
use crate::pipeline::{GlyphVertex, RRectVertex, RectVertex};
use crate::text::GlyphAtlas;

static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn warn_once(msg: &str) {
    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!("riode-wgpu: {msg} (further skips silent)");
    }
}

/// One draw op over the rect, glyph, or rounded-rect vertex buffers.
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
    /// Opaque base-color quads repainting the damaged area on partial
    /// frames. Always emitted first; drawn with blending disabled so they
    /// replace stale pixels instead of blending over them.
    BaseQuads { start: u32, count: u32 },
    Glyphs {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
        /// True when the current transform needs bilinear filtering
        /// (scale/skew/rotation); translation-only uses nearest.
        linear: bool,
    },
}

/// Emission context: transform/opacity stacks + batched verts + ordered ops.
pub struct Batch {
    tx: Vec<Transform>,
    opacity: Vec<f32>,
    pub clip: ClipStack,
    pub rect_verts: Vec<RectVertex>,
    pub rrect_verts: Vec<RRectVertex>,
    pub glyph_verts: Vec<GlyphVertex>,
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
            tx: vec![Transform::IDENTITY],
            opacity: vec![1.0],
            clip: ClipStack::default(),
            rect_verts: Vec::with_capacity(1024),
            rrect_verts: Vec::with_capacity(256),
            glyph_verts: Vec::with_capacity(1024),
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
        self.tx.clear();
        self.tx.push(Transform::from_scale(self.scale, self.scale));
        self.opacity.clear();
        self.opacity.push(1.0);
        self.clip.reset();
        self.rect_verts.clear();
        self.rrect_verts.clear();
        self.glyph_verts.clear();
        self.ops.clear();
        self.barrier = false;
    }

    /// Marks the start of a chunk's emission: the next run of any kind
    /// starts a fresh op. Called by the renderer per chunk (after base
    /// quads, before clips/base/commands) so snapshots stay complete.
    pub fn begin_chunk(&mut self) {
        self.barrier = true;
    }

    pub fn push_base(&mut self, t: Transform) {
        let top = *self.tx.last().unwrap();
        self.tx.push(top.multiply(&t));
    }

    pub fn pop_base(&mut self) {
        // reset() leaves exactly the root scale on the stack: pop anything
        // above it. (Both other backends truncate to entry depth; the old
        // `> 2` guard never fired, so every chunk inherited all previous
        // chunks' placements.)
        if self.tx.len() > 1 {
            self.tx.pop();
        }
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

    /// Pushes a node-local clip rect, transformed by the current matrix
    /// into device pixels (outward-rounded AABB: over-covers under skew,
    /// which clips less rather than more).
    /// For command-stream clips, which are relative to the current frame
    /// (node offsets precede the clip in the stream). Ignoring the
    /// transform intersects the rect near the origin and silently drops
    /// every op of nodes placed far from it.
    pub fn push_clip_local(&mut self, rect: &ResolvedRect, radius: Radius) {
        let t = self.cur_tx();
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
            &ResolvedRect::new(x0.floor(), y0.floor(), (x1 - x0).ceil(), (y1 - y0).ceil()),
            radius,
        );
    }

    fn cur_tx(&self) -> Transform {
        *self.tx.last().unwrap()
    }

    fn cur_op(&self) -> f32 {
        *self.opacity.last().unwrap()
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

    /// Translation + uniform positive scale (within epsilon): the SDF box
    /// stays axis-aligned and radii scale uniformly.
    fn tx_is_rigid(t: &Transform) -> bool {
        t.b.abs() < 1e-6 && t.c.abs() < 1e-6 && t.a > 1e-6 && (t.a - t.d).abs() < 1e-6
    }

    /// Emits a rounded-rect fill + stroke through the SDF pipeline.
    /// Returns false when the appearance or transform isn't representable
    /// (gradient paints, skewed transform); the caller falls back to a
    /// square solid base instead of dropping the rect.
    fn emit_rrect(
        &mut self,
        rect: &ResolvedRect,
        appearance: &riode::foundation::Appearance,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return true;
        }
        let t = self.cur_tx();
        if !Self::tx_is_rigid(&t) {
            return false;
        }
        let op = self.cur_op() * appearance.opacity;
        // Fill: solid only.
        let mut fill = [0.0, 0.0, 0.0, 0.0];
        match &appearance.fill {
            Some(Fill::Solid(c)) => {
                let a = (c.a * op).clamp(0.0, 1.0);
                if a > 0.0 {
                    fill = [c.r, c.g, c.b, a];
                }
            }
            Some(_) => return false,
            None => {}
        }
        // Stroke: solid only; ring placement follows alignment.
        let mut stroke = [0.0, 0.0, 0.0, 0.0];
        let mut width = 0.0f32;
        let mut expand = 0.0f32;
        if let Some(s) = &appearance.stroke {
            let Fill::Solid(c) = &s.fill else {
                return false;
            };
            let a = (c.a * op).clamp(0.0, 1.0);
            if a <= 0.0 || s.width <= 0.0 {
                // Invisible stroke: fill-only disc.
            } else {
                width = s.width * t.a;
                expand = match s.align {
                    StrokeAlign::Inside => 0.0,
                    StrokeAlign::Center => width * 0.5,
                    StrokeAlign::Outside => width,
                };
                stroke = [c.r, c.g, c.b, a];
            }
        }
        if fill[3] <= 0.0 && stroke[3] <= 0.0 {
            return true;
        }
        // Device-space fill box + uniformly scaled corner radii.
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
        // Cover the stroke ring + 1px AA fringe.
        let pad = expand + 1.5;
        let (qx0, qy0, qx1, qy1) = (dx0 - pad, dy0 - pad, dx1 + pad, dy1 + pad);
        let fbox = [dx0, dy0, dx1 - dx0, dy1 - dy0];
        let params = [width, expand, 0.0, 0.0];
        for (px, py) in [
            (qx0, qy0),
            (qx1, qy0),
            (qx0, qy1),
            (qx1, qy0),
            (qx1, qy1),
            (qx0, qy1),
        ] {
            self.rrect_verts.push(RRectVertex {
                pos: [px, py],
                fbox,
                radii,
                fill,
                stroke,
                params,
            });
        }
        self.push_rrect_run(6);
        true
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

    fn emit_solid(&mut self, rect: &ResolvedRect, color: Color) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let t = self.cur_tx();
        let a = (color.a * self.cur_op()).clamp(0.0, 1.0);
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

    #[allow(clippy::too_many_arguments)]
    fn emit_text(
        &mut self,
        origin: &Point,
        layout: &Arc<riode::text::layout::TextLayout>,
        color: Color,
        text_ctx: &TextContext,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
    ) {
        let a = (color.a * self.cur_op()).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let t = self.cur_tx();
        let c = [color.r, color.g, color.b, a];
        // Coverage quads always filter linearly: glyph rects land on
        // quarter-pixel boundaries (bin math below) and scaled metrics
        // divide by q, so nearest sampling snaps fractional texel taps to
        // blocky steps ("barely readable"). The 2px replicated atlas gutter
        // keeps linear border taps inside the owning cell.
        // Device scale: coverage rasters at size*q (engine scaled slots),
        // metrics below arrive in scaled px and are unscaled to layout
        // units before the root transform re-applies the scale.
        let q = quantize_raster_scale(self.scale);
        let sq16 = (q * 16.0).round() as u16;
        for line in layout.lines.iter() {
            // Size-split transfer: hinted small text composites raw
            // coverage; display type remaps through the gamma LUT.
            let gam = if line.font_size <= 16.0 { 0.0 } else { 1.0 };
            for g in line.glyphs.iter() {
                let pen_x = origin.x + g.point.x + line.align_offset;
                let pen_y = origin.y + g.point.y;
                let placed = t.transform_point(Point::new(pen_x, pen_y));
                // Subpixel policy (mirrors the tiny-skia reference): the X bin
                // selects only the *raster variant* (nearest quarter) while
                // *placement stays exact* — quantizing X placement drops a
                // per-glyph residual, and glyphs crossing bin edges on
                // different frames visibly drift at different speeds.
                // Y is frozen at bin zero, exactly like the CPU path: at
                // hinted sizes the hinter grid-fits vertical stems, so every
                // Y phase change re-fits the outline (morphing/popping per
                // glyph on different frames). The frozen raster never morphs;
                // the exact Y fraction rides the quad position and dissolves
                // into linear filtering, so the run glides uniformly.
                let fx = placed.x - placed.x.floor();
                let binx = (((fx * 4.0) + 0.5).floor() as i32).clamp(0, 3);
                let phase = (binx as u8) & 3;
                // Single raster: bitmap + bearing together (no TOCTOU).
                // Metrics arrive in scaled px (q==1.0 at unity scale).
                let rastered =
                    text_ctx.raster_glyph_scaled(g.cache_key, phase, self.scale, |bmp| {
                        bmp.and_then(|b| {
                            if b.width == 0 || b.height == 0 {
                                return None;
                            }
                            let entry = atlas.get(g.cache_key, phase, sq16).or_else(|| {
                                atlas.insert(
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
                // device fractions, not the bin centers: X removes fx (the
                // bin's baked offset plus the <=1/8px leftover, which rides
                // the fractional quad position through linear filtering);
                // Y removes nothing but the bearing — the baked Y offset is
                // always zero (frozen bin), exactly like the reference
                // gy_exact = dev_y - top, and fy rides the quad position.
                // UVs arrive edge-exact from the atlas (2px gutter, no inset).
                let (u0, v0) = (e.u0, e.v0);
                let (u1, v1) = (e.u1, e.v1);
                let (bw, bh) = (e.w as f32 / q, e.h as f32 / q);
                let lx0 = pen_x + (bl as f32 - fx) / q;
                let ly0 = pen_y - bt as f32 / q;
                // Translation-only collapses to (dev - fx + left,
                // dev.y - fy - top): the exact pen, so every `.` press moves
                // each glyph by exactly 1/8px like the reference.
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
                        gam,
                    });
                }
                self.push_glyph_run(6, true);
            }
        }
    }

    pub fn commands(
        &mut self,
        commands: &[SceneCommand],
        text_ctx: &TextContext,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
    ) {
        for cmd in commands {
            match cmd {
                SceneCommand::PushTransform(t) => {
                    let top = self.cur_tx();
                    self.tx.push(top.multiply(t));
                }
                SceneCommand::PopTransform => {
                    if self.tx.len() > 1 {
                        self.tx.pop();
                    }
                }
                SceneCommand::PushOffset(p) => {
                    let top = self.cur_tx();
                    self.tx
                        .push(top.multiply(&Transform::from_translation(p.x, p.y)));
                }
                SceneCommand::PopOffset => {
                    if self.tx.len() > 1 {
                        self.tx.pop();
                    }
                }
                SceneCommand::PushOpacity(o) => {
                    let v = self.cur_op() * o.clamp(0.0, 1.0);
                    self.opacity.push(v);
                }
                SceneCommand::PopOpacity => {
                    if self.opacity.len() > 1 {
                        self.opacity.pop();
                    }
                }
                SceneCommand::PushClip { rect, radius } => self.push_clip_local(rect, *radius),
                SceneCommand::PopClip => self.clip.pop(),
                SceneCommand::BeginLayer { .. } => {
                    warn_once("BeginLayer offscreen unsupported, drawing inline")
                }
                SceneCommand::EndLayer { .. } => {}
                SceneCommand::DrawRect { rect, appearance } => {
                    let plain_square = matches!(
                        appearance.radius,
                        Radius::Scalar(v) if v <= 0.0
                    ) && appearance.stroke.is_none();
                    match &appearance.fill {
                        Some(Fill::Solid(c)) => {
                            let mut col = *c;
                            col.a *= appearance.opacity;
                            if plain_square {
                                self.emit_solid(rect, col);
                            } else if !self.emit_rrect(rect, appearance) {
                                // Skewed transform or gradient stroke:
                                // square solid base keeps layout visible.
                                warn_once("rrect unsupported, square fallback");
                                self.emit_solid(rect, col);
                            }
                        }
                        Some(_) => {
                            // Gradient + radius/stroke: SDF path needs a
                            // solid paint; skip rather than misrender.
                            if plain_square {
                                warn_once("gradient fill unsupported, skipped");
                            } else if !self.emit_rrect(rect, appearance) {
                                warn_once("gradient rrect unsupported, skipped");
                            }
                        }
                        None => {
                            // Stroke-only ring.
                            if !plain_square && !self.emit_rrect(rect, appearance) {
                                warn_once("stroke-only rrect unsupported, skipped");
                            }
                        }
                    }
                }
                SceneCommand::DrawImage { .. } => warn_once("DrawImage unsupported, skipped"),
                SceneCommand::DrawSvg { .. } => warn_once("DrawSvg unsupported, skipped"),
                SceneCommand::DrawShadow { .. } => warn_once("DrawShadow unsupported, skipped"),
                SceneCommand::DrawText {
                    origin,
                    layout,
                    color,
                } => self.emit_text(origin, layout, *color, text_ctx, atlas, queue),
            }
        }
    }
}

impl Default for Batch {
    fn default() -> Self {
        Self::new()
    }
}
