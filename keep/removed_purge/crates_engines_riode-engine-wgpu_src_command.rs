// Single responsibility: SceneCommand -> batched layer, glyph and image
// vertex emission.

use riode::foundation::{Color, Point, ResolvedRect, Transform};
use riode::scene::command::SceneCommand;
use riode::scene::{
    quantize_quarter, quantize_y, PlacedClip, PlacedImage, PlacedLayer, PlacedSvg, PlacedText,
    SceneSink,
};
use riode::text::{quantize_raster_scale, TextContext};
use std::sync::Arc;

use crate::clip::ClipStack;
use crate::image::ImageGpu;
use crate::pipeline::{GlyphVertex, ImageVertex, MaterialPassGpu, MaterialUniformsGpu, RectVertex};
use crate::text::GlyphAtlas;

static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn warn_once(msg: &str) {
    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!("riode-wgpu: {msg} (further skips silent)");
    }
}

/// One draw op over the base, glyph, image, or layer-composite vertex
/// buffers. The legacy direct rect/rrect/shadow draws are gone: a node
/// paints through its material layer.
#[derive(Clone)]
pub enum Op {
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
    /// Opens a material layer.
    BeginLayer {
        region: ResolvedRect,
        card_rect: ResolvedRect,
        params: u32,
        material: Option<u32>,
    },
    /// Closes the innermost material layer: the renderer runs the
    /// material pass over the layer's snapshot, then draws the composite
    /// quad `start..start + count` (rect_verts, the region) onto the
    /// parent through `scissor` (ancestor clips; in-layer clips died with
    /// the layer's balanced pops).
    EndLayer {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
    },
}

/// One open layer, emission-side: the offscreen region + filter-params
/// index (`Some`), or `None` for a plain clip layer.
struct LayerStackEntry {
    region: Option<(ResolvedRect, u32)>,
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
/// (`SceneSink`), and every item already carries its absolute transform.
/// The engine composes its root device scale around each item — the only
/// matrix it owns.
pub struct Batch {
    pub clip: ClipStack,
    pub rect_verts: Vec<RectVertex>,
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
    pub material_params: Vec<crate::pipeline::MaterialPassGpu>,
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
            glyph_verts: Vec::with_capacity(1024),
            image_verts: Vec::with_capacity(256),
            image_binds: Vec::new(),
            ops: Vec::new(),
            scale: 1.0,
            barrier: false,
            filter_params: Vec::new(),
            material_params: Vec::new(),
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
        self.glyph_verts.clear();
        self.image_verts.clear();
        self.image_binds.clear();
        self.ops.clear();
        self.barrier = false;
        self.filter_params.clear();
        self.material_params.clear();
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
    /// Clips are square — `radius` was purged with the legacy paint path.
    pub fn push_clip_device(&mut self, rect: &ResolvedRect) {
        let s = self.scale;
        self.clip.push(&ResolvedRect::new(
            rect.x * s,
            rect.y * s,
            rect.width * s,
            rect.height * s,
        ));
    }

    /// Pushes a node-local clip rect, transformed by the composed device
    /// matrix into device pixels (AABB: over-covers under skew, which
    /// clips less rather than more).
    /// For command-stream clips, which are relative to the current frame
    /// (node offsets precede the clip in the stream). Ignoring the
    /// transform intersects the rect near the origin and silently drops
    /// every op of nodes placed far from it.
    pub fn push_clip_local(&mut self, rect: &ResolvedRect, t: Transform) {
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
        self.clip
            .push(&riode::scene::integerize_damage(&ResolvedRect::new(
                x0,
                y0,
                x1 - x0,
                y1 - y0,
            )));
    }

    /// Translation + uniform positive scale (within epsilon): the SDF box
    /// stays axis-aligned and radii scale uniformly.
    fn tx_is_rigid(t: &Transform) -> bool {
        t.b.abs() < 1e-6 && t.c.abs() < 1e-6 && t.a > 1e-6 && (t.a - t.d).abs() < 1e-6
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

    /// One base-color quad in device px, same replace pipeline as
    /// [`Self::emit_damage_base`]. Merges into a trailing `BaseQuads`
    /// op so consecutive base rects share one draw call.
    pub fn emit_base_rect_dev(&mut self, r: &ResolvedRect, base: Color) {
        let (x0, y0) = (r.x, r.y);
        let (x1, y1) = (r.x + r.width, r.y + r.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let c = [base.r, base.g, base.b, base.a];
        for (px, py) in [(x0, y0), (x1, y0), (x0, y1), (x1, y0), (x1, y1), (x0, y1)] {
            self.rect_verts.push(RectVertex { pos: [px, py], color: c });
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
        self.batch.push_clip_local(&clip.rect, t);
    }

    fn pop_clip(&mut self) {
        self.batch.clip.pop();
    }

    /// Opens a material layer: the offscreen region is the layer rect
    /// grown by the chain's pad (3×sigma), the composite material runs
    /// over it at `end_layer`, and the content is captured into the same
    /// offscreen. No filter stack, no shadows, no radii — those were
    /// purged with the legacy paint path.
    fn begin_layer(&mut self, layer: &PlacedLayer) {
        let mut material_sigma = 0.0f32;
        let mut material_pad = 0.0f32;
        let mut material_idx: Option<u32> = None;

        // Device-space content box
        let t = self.batch.device_tx(&layer.transform);
        let mut x0 = f32::INFINITY;
        let mut y0 = f32::INFINITY;
        let mut x1 = f32::NEG_INFINITY;
        let mut y1 = f32::NEG_INFINITY;
        for (px, py) in [
            (layer.rect.x, layer.rect.y),
            (layer.rect.x + layer.rect.width, layer.rect.y),
            (layer.rect.x, layer.rect.y + layer.rect.height),
            (layer.rect.x + layer.rect.width, layer.rect.y + layer.rect.height),
        ] {
            let p = t.transform_point(Point::new(px, py));
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        // Material region: content box + pad (3×sigma), so the material's
        // own backdrop sampling has room around the silhouette.
        if let Some(mat) = &layer.material {
            let (idx, params) = self.batch.material_params.len() as u32, ();
            let _ = idx;
            let idx = self.batch.material_params.len() as u32;
            let mut block = [0.0f32; riode::foundation::K_MODULAR_FLOATS];
            let geo = riode::foundation::Geo::from_rect(&layer.rect);
            for item in mat.items() {
                item.write_block(&geo, &mut block, self.batch.scale());
            }
            material_sigma = mat.sigma();
            material_pad = mat.pad();
            material_idx = Some(idx);
            self.batch.material_params.push(MaterialPassGpu {
                idx,
                shader: mat.shader(),
                block,
            });
            let _ = params;
        }

        let region = ResolvedRect::new(
            x0 - material_pad,
            y0 - material_pad,
            (x1 - x0) + material_pad * 2.0,
            (y1 - y0) + material_pad * 2.0,
        );
        let card_rect = ResolvedRect::new(x0, y0, x1 - x0, y1 - y0);

        self.batch.begin_layer(region, card_rect, material_idx);

        // Capture the parent behind the region so the material can sample
        // the backdrop and the content composite can blend over it.
        let region_dev = ResolvedRect::new(
            region.x * self.batch.scale(),
            region.y * self.batch.scale(),
            region.width * self.batch.scale(),
            region.height * self.batch.scale(),
        );
        self.batch.push_layer_targets(region_dev, material_sigma);
    }

    /// Closes the innermost material layer: run the material pass over the
    /// layer's snapshot into `gt.out`, then composite `start..start+count`
    /// (the region quad) onto the parent through `scissor`.
    fn end_layer(&mut self) {
        let Some(entry) = self.batch.layer_stack.pop() else {
            return;
        };
        let Some((region, mat_idx)) = entry.region else {
            return;
        };
        let scissor = self.batch.clip.current();
        let start = self.batch.rect_verts.len() as u32;
        self.batch.emit_layer_quad(&region, mat_idx);
        let count = self.batch.rect_verts.len() as u32 - start;
        self.batch.ops.push(Op::EndLayer {
            start,
            count,
            scissor,
        });
    }

    fn image(&mut self, i: &PlacedImage) {
        if let Some(gpu) = self.images.get(i, self.queue, self.device) {
            let t = self.batch.device_tx(&i.transform);
            self.batch.emit_image_quad(&i.rect, gpu, t, 1.0);
        }
    }

    fn svg(&mut self, s: &PlacedSvg) {
        if let Some((gpu, w, h)) = self.svgs.get(s, self.device, self.queue) {
            let rect = ResolvedRect::new(s.origin.x, s.origin.y, w, h);
            let t = self.batch.device_tx(&s.transform);
            self.batch.emit_image_quad(&rect, gpu, t, 1.0);
        }
    }

    fn text(&mut self, t: &PlacedText) {
        let (atlas, scale) = {
            let t = quantize_raster_scale(
                self.batch.scale(),
                t.layout.size,
                t.transform.a,
                t.transform.d,
            );
            (t.0, t.1)
        };
        let start = self.batch.glyph_verts.len() as u32;
        self.batch.emit_text(
            &t.layout,
            &t.transform,
            atlas,
            scale,
            &self.text_ctx,
            self.atlas,
            quantize_quarter,
            quantize_y,
            t.color,
            1.0,
        );
        let count = self.batch.glyph_verts.len() as u32 - start;
        if count > 0 {
            self.batch.push_glyph_run(count, true);
        }
    }
}

impl Default for Batch {
    fn default() -> Self {
        Self::new()
    }
}
