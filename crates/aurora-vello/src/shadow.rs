// Single responsibility: Layered elevation-based shadow rasterization and caching for Vello.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect};
use vello::peniko::{
    BlendMode, Blob, Brush, Compose, Fill as VelloFillRule, Format as PenikoFormat,
    Image as PenikoImage, Mix,
};
use vello::Scene as VelloScene;

use crate::shader::to_vello_color;
use aurora::foundation::{
    BlendMode as ShadowBlend, LightEnvironment, Radius, ResolvedRect, Shadow,
};
use aurora::render::blur::box_blur_alpha_with_falloff;

const MAX_CACHED_SHADOWS: usize = 64;

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
struct ShadowKey {
    radius_bits: u32,
    softness_bits: u32,
    extent_bits: u32,
    spread_bits: u32,
    falloff_tag: u8,
    color_bits: u32,
}

struct ShadowPatch {
    corner: Arc<PenikoImage>,
    edge_h: Arc<PenikoImage>,
    edge_v: Arc<PenikoImage>,
    cs: f64,
}

/// Retained 9-patch texture cache for layered soft shadows in Vello.
#[derive(Default)]
pub struct VelloShadowCache {
    cache: FxHashMap<ShadowKey, ShadowPatch>,
    /// Last-use clock per entry (stalest evicted, never arbitrary).
    used: FxHashMap<ShadowKey, u64>,
    clock: u64,
}

impl VelloShadowCache {
    /// Constructs a clean Vello shadow cache.
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
            used: FxHashMap::default(),
            clock: 0,
        }
    }

    /// Draws layered 9-patch shadows transformed through the affine stack.
    ///
    /// `Multiply` layers are wrapped in a multiply-composite layer since
    /// `draw_image` carries no per-draw blend mode in Vello 0.2.
    pub fn draw(
        &mut self,
        scene: &mut VelloScene,
        rect: &ResolvedRect,
        radius: Radius,
        shadow: &Shadow,
        light: &LightEnvironment,
        tx: Affine,
    ) {
        if shadow.layers().is_empty() {
            return;
        }
        // See tiny-skia: Alpha falls back to the box path until accelerated.
        if matches!(shadow.shape, aurora::foundation::ShadowShape::Alpha) {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!("ShadowShape::Alpha falls back to Box until accelerated");
            }
        }
        let intensity = light.intensity;
        if intensity <= 0.0 {
            return;
        }

        let r_scalar = match radius {
            Radius::Scalar(v) => v.max(0.0),
            Radius::Max => (rect.width.min(rect.height) * 0.5).max(0.0),
            Radius::Corners { .. } => radius.resolve(rect.width, rect.height),
        };

        // Largest extent first so tighter layers paint over softer ones.
        let mut order: Vec<usize> = (0..shadow.layers().len()).collect();
        order.sort_by(|&a, &b| {
            shadow.layers()[b]
                .extent
                .partial_cmp(&shadow.layers()[a].extent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for i in order {
            let layer = &shadow.layers()[i];
            let layer_alpha = (intensity * shadow.color.a * layer.opacity).clamp(0.0, 1.0);
            if layer_alpha <= 0.0 || layer.extent <= 0.0 {
                continue;
            }
            let (offset_x, offset_y) = shadow.layer_offset(light, layer);
            let blur_r = layer.softness.max(0.5);
            let pad = shadow.layer_pad(layer);

            let key = ShadowKey {
                radius_bits: r_scalar.to_bits(),
                softness_bits: blur_r.to_bits(),
                extent_bits: pad.to_bits(),
                spread_bits: shadow.spread.to_bits(),
                falloff_tag: layer.falloff.tag(),
                color_bits: (shadow.color.r.to_bits()
                    ^ shadow.color.g.to_bits()
                    ^ shadow.color.b.to_bits()),
            };

            if !self.cache.contains_key(&key) {
                while self.cache.len() >= MAX_CACHED_SHADOWS {
                    let victim = self.used.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k);
                    match victim {
                        Some(k) => {
                            self.cache.remove(&k);
                            self.used.remove(&k);
                        }
                        None => break,
                    }
                }
                let mut tinted = shadow.color;
                tinted.a = 1.0;
                let lut = layer.falloff.lut();
                let patch =
                    Self::generate_patch(r_scalar, blur_r, pad, shadow.spread, &lut, &tinted);
                self.cache.insert(key, patch);
            }
            self.clock += 1;
            self.used.insert(key, self.clock);

            let p = self.cache.get(&key).unwrap();
            let (ox, oy) = (
                (rect.x + offset_x - pad) as f64,
                (rect.y + offset_y - pad) as f64,
            );
            let (total_w, total_h) = (
                (rect.width + 2.0 * pad) as f64,
                (rect.height + 2.0 * pad) as f64,
            );
            let cs = p.cs;

            // Per-draw opacity does not exist on draw_image/fill in Vello
            // 0.2, so every layer paints at full texture alpha inside an
            // opacity layer; multiply additionally swaps the composite mode.
            let layer_rect = KurboRect::new(ox, oy, ox + total_w, oy + total_h);
            let mode = match shadow.blend {
                ShadowBlend::Normal => BlendMode::default(),
                ShadowBlend::Multiply => BlendMode::new(Mix::Multiply, Compose::SrcOver),
            };
            scene.push_layer(mode, layer_alpha, tx, &layer_rect);
            self.draw_patch(scene, p, ox, oy, total_w, total_h, cs, shadow, 1.0, tx);
            scene.pop_layer();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_patch(
        &self,
        scene: &mut VelloScene,
        p: &ShadowPatch,
        ox: f64,
        oy: f64,
        total_w: f64,
        total_h: f64,
        cs: f64,
        shadow: &Shadow,
        alpha: f32,
        tx: Affine,
    ) {
        // 4 Corners
        scene.draw_image(p.corner.as_ref(), tx * Affine::translate((ox, oy)));
        scene.draw_image(
            p.corner.as_ref(),
            tx * Affine::translate((ox + total_w, oy)) * Affine::scale_non_uniform(-1.0, 1.0),
        );
        scene.draw_image(
            p.corner.as_ref(),
            tx * Affine::translate((ox, oy + total_h)) * Affine::scale_non_uniform(1.0, -1.0),
        );
        scene.draw_image(
            p.corner.as_ref(),
            tx * Affine::translate((ox + total_w, oy + total_h))
                * Affine::scale_non_uniform(-1.0, -1.0),
        );

        // Stretched Edges
        let edge_w = (total_w - 2.0 * cs).max(0.0);
        let edge_h = (total_h - 2.0 * cs).max(0.0);
        if edge_w > 0.0 {
            scene.draw_image(
                p.edge_h.as_ref(),
                tx * Affine::translate((ox + cs, oy)) * Affine::scale_non_uniform(edge_w, 1.0),
            );
            scene.draw_image(
                p.edge_h.as_ref(),
                tx * Affine::translate((ox + cs, oy + total_h))
                    * Affine::scale_non_uniform(edge_w, -1.0),
            );
        }
        if edge_h > 0.0 {
            scene.draw_image(
                p.edge_v.as_ref(),
                tx * Affine::translate((ox, oy + cs)) * Affine::scale_non_uniform(1.0, edge_h),
            );
            scene.draw_image(
                p.edge_v.as_ref(),
                tx * Affine::translate((ox + total_w, oy + cs))
                    * Affine::scale_non_uniform(-1.0, edge_h),
            );
        }

        // Center solid fill
        if edge_w > 0.0 && edge_h > 0.0 {
            let center_rect =
                KurboRect::new(ox + cs, oy + cs, ox + total_w - cs, oy + total_h - cs);
            scene.fill(
                VelloFillRule::NonZero,
                tx,
                &Brush::Solid(to_vello_color(shadow.color, alpha)),
                None,
                &center_rect,
            );
        }
    }

    fn generate_patch(
        radius: f32,
        softness: f32,
        pad: f32,
        spread: f32,
        falloff_lut: &[u8; 256],
        color: &aurora::foundation::Color,
    ) -> ShadowPatch {
        let cs = ((radius + pad).ceil() as usize).max(4);
        let dim = cs * 2;
        let buf_len = dim * dim;
        let (mut a, mut b, mut temp) = (vec![0u8; buf_len], vec![0u8; buf_len], vec![0u8; buf_len]);

        // Spread inflates the occluder (Vello keeps the solid-quadrant
        // model with blur-rounded corners, matching its existing path).
        let inset = pad - spread;
        for y in 0..dim {
            for x in 0..dim {
                let (px, py) = (x as f32, y as f32);
                if px >= inset && py >= inset {
                    a[y * dim + x] = 255;
                }
            }
        }
        box_blur_alpha_with_falloff(
            &a,
            &mut b,
            &mut temp,
            dim,
            dim,
            softness.round() as usize,
            falloff_lut,
        );

        // Textures bake the tint at full alpha; per-layer alpha rides on
        // the enclosing opacity layer so corners, edges, and center agree.
        let (cr, cg, cb) = (
            (color.r * 255.0).round() as u32,
            (color.g * 255.0).round() as u32,
            (color.b * 255.0).round() as u32,
        );
        let to_rgba = |alpha: u8| -> [u8; 4] {
            let a = alpha as u32;
            [
                ((cr * a + 128) >> 8) as u8,
                ((cg * a + 128) >> 8) as u8,
                ((cb * a + 128) >> 8) as u8,
                a as u8,
            ]
        };

        let mut corner_bytes = Vec::with_capacity(cs * cs * 4);
        for y in 0..cs {
            for x in 0..cs {
                corner_bytes.extend_from_slice(&to_rgba(b[y * dim + x]));
            }
        }

        let mut edge_h_bytes = Vec::with_capacity(cs * 4);
        for y in 0..cs {
            edge_h_bytes.extend_from_slice(&to_rgba(b[y * dim + cs]));
        }

        let mut edge_v_bytes = Vec::with_capacity(cs * 4);
        for x in 0..cs {
            edge_v_bytes.extend_from_slice(&to_rgba(b[cs * dim + x]));
        }

        ShadowPatch {
            corner: Arc::new(PenikoImage::new(
                Blob::from(corner_bytes),
                PenikoFormat::Rgba8,
                cs as u32,
                cs as u32,
            )),
            edge_h: Arc::new(PenikoImage::new(
                Blob::from(edge_h_bytes),
                PenikoFormat::Rgba8,
                1,
                cs as u32,
            )),
            edge_v: Arc::new(PenikoImage::new(
                Blob::from(edge_v_bytes),
                PenikoFormat::Rgba8,
                cs as u32,
                1,
            )),
            cs: cs as f64,
        }
    }
}
