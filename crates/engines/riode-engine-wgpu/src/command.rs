// Single responsibility: SceneCommand -> batched layer, glyph and image vertex emission.

use riode::foundation::{Color, LayerPaint, Point, ResolvedRect, Transform};
use riode::scene::command::SceneCommand;
use riode::scene::{
    quantize_quarter, quantize_y, PlacedClip, PlacedImage, PlacedLayer, PlacedSvg, PlacedText,
    SceneSink,
};
use riode::text::{quantize_raster_scale, TextContext};
use std::sync::Arc;

use crate::clip::ClipStack;
use crate::image::ImageGpu;
use crate::pipeline::{GlyphVertex, ImageVertex, RectVertex};
use crate::text::GlyphAtlas;

/// One draw op over the base, glyph, image, or layer-composite vertex buffers.
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
    /// Opens an offscreen layer carrying the node's paint bundle.
    BeginLayer {
        region: ResolvedRect,
        paint: LayerPaint,
    },
    /// Closes the innermost layer: draws overlay before close, and
    /// composites through the effect shader onto the parent.
    EndLayer {
        start: u32,
        count: u32,
        scissor: Option<ResolvedRect>,
        region: ResolvedRect,
        paint: LayerPaint,
    },
}

/// One open layer, emission-side: the offscreen region and its paint bundle.
struct LayerStackEntry {
    region: ResolvedRect,
    paint: LayerPaint,
}

/// Emission context: batched verts + ordered ops.
pub struct Batch {
    pub clip: ClipStack,
    pub rect_verts: Vec<RectVertex>,
    pub glyph_verts: Vec<GlyphVertex>,
    pub image_verts: Vec<ImageVertex>,
    pub image_binds: Vec<Arc<ImageGpu>>,
    pub ops: Vec<Op>,
    scale: f32,
    barrier: bool,
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
            layer_stack: Vec::new(),
        }
    }

    pub fn set_scale(&mut self, dsf: f32) {
        self.scale = dsf.clamp(0.5, 4.0);
    }

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
        if !self.layer_stack.is_empty() {
            tracing::warn!(
                "riode-wgpu: {} unclosed filter layer(s) at frame end (unbalanced BeginLayer/EndLayer)",
                self.layer_stack.len()
            );
            self.layer_stack.clear();
        }
    }

    pub fn begin_chunk(&mut self) {
        self.barrier = true;
    }

    fn device_tx(&self, t: &Transform) -> Transform {
        Transform::from_scale(self.scale, self.scale).multiply(t)
    }

    pub fn push_clip_device(&mut self, rect: &ResolvedRect) {
        let s = self.scale;
        self.clip.push(&ResolvedRect::new(
            rect.x * s,
            rect.y * s,
            rect.width * s,
            rect.height * s,
        ));
    }

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

    fn emit_image_quad(&mut self, rect: &ResolvedRect, gpu: Arc<ImageGpu>, t: Transform, op: f32) {
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

    pub fn emit_base_rect_dev(&mut self, r: &ResolvedRect, base: Color) {
        let (x0, y0) = (r.x, r.y);
        let (x1, y1) = (r.x + r.width, r.y + r.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let c = [base.r, base.g, base.b, base.a];
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
        let q = quantize_raster_scale(self.scale);
        let sq16 = (q * 16.0).round() as u16;
        for line in layout.lines.iter() {
            for g in line.glyphs.iter() {
                let pen_x = origin.x + g.point.x + line.align_offset;
                let pen_y = origin.y + g.point.y;
                let placed = t.transform_point(Point::new(pen_x, pen_y));
                let (bx, qx) = quantize_quarter(placed.x);
                let fx = placed.x - bx as f32;
                let (qy, fy) = if riode::text::baseline_snap_y() {
                    let by = placed.y.round() as i32;
                    (0u8, placed.y - by as f32)
                } else {
                    let (by, qy) = quantize_y(placed.y);
                    (qy, placed.y - by as f32)
                };
                let phase = (qx as u8) | ((qy as u8) << 2);
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

    fn begin_layer(&mut self, layer: &PlacedLayer) {
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
        let region = ResolvedRect::new(x0, y0, x1 - x0, y1 - y0);

        self.batch.ops.push(Op::BeginLayer {
            region,
            paint: layer.paint.clone(),
        });
        self.batch.layer_stack.push(LayerStackEntry {
            region,
            paint: layer.paint.clone(),
        });
    }

    fn end_layer(&mut self) {
        let Some(entry) = self.batch.layer_stack.pop() else {
            return;
        };
        let region = entry.region;
        let scissor = self.batch.clip.current();

        // Expand quad if an effect is present so distortions don't clip at layout edges
        let margin = if entry.paint.effect.is_some() {
            32.0
        } else {
            0.0
        };
        let (x0, y0) = (region.x - margin, region.y - margin);
        let (x1, y1) = (
            region.x + region.width + margin,
            region.y + region.height + margin,
        );

        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let start = self.batch.rect_verts.len() as u32;
        for (px, py) in [(x0, y0), (x1, y0), (x0, y1), (x1, y0), (x1, y1), (x0, y1)] {
            self.batch.rect_verts.push(RectVertex {
                pos: [px, py],
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
        let count = self.batch.rect_verts.len() as u32 - start;
        self.batch.ops.push(Op::EndLayer {
            start,
            count,
            scissor,
            region: entry.region,
            paint: entry.paint,
        });
    }

    fn image(&mut self, i: &PlacedImage) {
        if let Some(gpu) = self.images.get_or_insert(self.device, self.queue, i.image) {
            let t = self.batch.device_tx(&i.transform);
            self.batch.emit_image_quad(&i.rect, gpu, t, 1.0);
        }
    }

    fn svg(&mut self, s: &PlacedSvg) {
        if let Some(gpu) = self.svgs.get_or_insert(
            self.device,
            self.queue,
            self.images,
            s.graphic,
            &s.rect,
            self.batch.scale(),
        ) {
            let t = self.batch.device_tx(&s.transform);
            self.batch.emit_image_quad(&s.rect, gpu, t, 1.0);
        }
    }

    fn text(&mut self, t: &PlacedText) {
        let start = self.batch.glyph_verts.len() as u32;
        let tx = self.batch.device_tx(&t.transform);
        self.batch.emit_text(
            &t.origin,
            t.layout,
            t.color,
            self.text_ctx,
            self.atlas,
            self.queue,
            tx,
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
