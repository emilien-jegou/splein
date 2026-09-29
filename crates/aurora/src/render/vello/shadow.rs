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
