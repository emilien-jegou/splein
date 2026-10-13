// Single responsibility: Layered elevation-based shadow rasterization with LRU 9-patch caching.

pub mod direct;
pub mod patch;

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tiny_skia::{Mask, Pixmap, PixmapMut, Transform};

use aurora::foundation::{LightEnvironment, Radius, ResolvedRect, Shadow};
use direct::render_direct_blur;
use patch::{blit_9patch, generate_patch, layer_geometry, ShadowPatch, ShadowPatchKey};

const MAX_SHADOW_PATCHES: usize = 64;

/// Byte budget for patch masks: count caps alone admit 64 adversarial
/// patches (~2 MB each at huge softness → ~128 MB). Typical entries are
/// ~10 KB; oversize singles bypass like everywhere else.
const MAX_SHADOW_BYTES: usize = 8 << 20;

/// Software shadow rasterizer caching 9-patch blurred corner templates.
///
/// Renders each [`Shadow`] layer back-to-front (largest extent first) with
/// its own falloff LUT, compositing with the shadow's blend mode.
pub struct ShadowRasterizer {
    scratch_a: Vec<u8>,
    scratch_b: Vec<u8>,
    scratch_temp: Vec<u8>,
    patch_cache: FxHashMap<ShadowPatchKey, ShadowPatch>,
    /// Last-use clock per patch: eviction drops the stalest entry, not an
    /// arbitrary one (hash-order drops thrashed hot patches under pressure).
    patch_used: FxHashMap<ShadowPatchKey, u64>,
    patch_clock: u64,
    patch_bytes: usize,
    /// Reused direct-path temp pixmap (M4): exact-size match only, since
    /// `draw_pixmap` blits the whole buffer.
    direct_scratch: Option<Pixmap>,
}

/// Grow-only scratch would pin a once-huge frame forever (M6): shrink back
/// when retained buffers exceed 4x the need, mirroring the tile guard.
/// Shared by the patch and direct paths (different size regimes).
pub(crate) fn fit_scratch_buffers(
    bufs: [&mut Vec<u8>; 3],
    need: usize,
    fill: u8,
) {
    for buf in bufs {
        if buf.len() > need.saturating_mul(4) {
            buf.clear();
            buf.reserve_exact(need);
        }
        buf.resize(need, fill);
    }
}

impl ShadowRasterizer {
    /// Constructs a clean shadow rasterizer.
    pub fn new() -> Self {
        Self {
            scratch_a: Vec::with_capacity(128 * 128),
            scratch_b: Vec::with_capacity(128 * 128),
            scratch_temp: Vec::with_capacity(128 * 128),
            patch_cache: FxHashMap::default(),
            patch_used: FxHashMap::default(),
            patch_clock: 0,
            patch_bytes: 0,
            direct_scratch: None,
        }
    }

    fn touch_patch(&mut self, key: ShadowPatchKey) {
        self.patch_clock += 1;
        self.patch_used.insert(key, self.patch_clock);
    }

    fn evict_oldest_patch(&mut self) {        let victim = self
            .patch_used
            .iter()
            .min_by_key(|(_, t)| **t)
            .map(|(k, _)| *k);
        if let Some(k) = victim {
            if let Some(old) = self.patch_cache.remove(&k) {
                self.patch_bytes = self.patch_bytes.saturating_sub(old.mask.len());
            }
            self.patch_used.remove(&k);
        }
    }

    /// Renders analytical soft shadows or direct blurred fallbacks per layer.
    pub fn render(
        &mut self,
        pixmap: &mut PixmapMut,
        rect: &ResolvedRect,
        radius: Radius,
        shadow: &Shadow,
        light: &LightEnvironment,
        opacity: f32,
        transform: Transform,
        clip: Option<&Mask>,
    ) {
        if shadow.layers().is_empty() {
            return;
        }
        // Alpha-silhouette shadows are not accelerated yet; the 9-patch box
        // path below is the graceful fallback. Warn once, not per frame.
        if matches!(shadow.shape, aurora::foundation::ShadowShape::Alpha) {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!("ShadowShape::Alpha falls back to Box until accelerated");
            }
        }
        let total_opacity = opacity * light.intensity;
        if total_opacity <= 0.0 {
            return;
        }

        let r_scalar = match radius {
            Radius::Scalar(v) => v.max(0.0),
            Radius::Max => (rect.width.min(rect.height) * 0.5).max(0.0),
            Radius::Corners { .. } => radius.resolve(rect.width, rect.height),
        };

        // Largest extent first so tighter layers paint over softer ones.
        // Stack-inline: 1-3 layers typical, heap only past 4 (M8).
        let mut order: SmallVec<[usize; 4]> = (0..shadow.layers().len()).collect();
        order.sort_by(|&a, &b| {
            shadow.layers()[b]
                .extent
                .partial_cmp(&shadow.layers()[a].extent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for i in order {
            let layer = &shadow.layers()[i];
            let layer_opacity = total_opacity * shadow.color.a * layer.opacity;
            if layer_opacity <= 0.0 || layer.extent <= 0.0 {
                continue;
            }
            let (offset_x, offset_y) = shadow.layer_offset(light, layer);
            let blur_r = layer.softness.max(0.5);
            let pad = shadow.layer_pad(layer);
            // Slice past the inward blur support (see layer_geometry).
            let (kernel_pad, corner_size, patch_dim) = layer_geometry(r_scalar, shadow, layer);

            // The 9-patch blit only applies the translation components of the
            // transform, so skewed (rotated) shadows fall back to the direct path,
            // which composites the blurred shape through the full transform.
            let is_too_small = rect.width < (patch_dim as f32)
                || rect.height < (patch_dim as f32)
                || transform.has_skew();
            if is_too_small {
                render_direct_blur(
                    &mut self.scratch_a,
                    &mut self.scratch_b,
                    &mut self.scratch_temp,
                    &mut self.direct_scratch,
                    pixmap,
                    rect,
                    r_scalar,
                    offset_x,
                    offset_y,
                    blur_r,
                    pad,
                    shadow.spread,
                    &layer.falloff.lut(),
                    shadow.color,
                    layer_opacity,
                    shadow.blend,
                    transform,
                    clip,
                );
                continue;
            }

            let key = ShadowPatchKey {
                radius_bits: r_scalar.to_bits(),
                blur_bits: blur_r.to_bits(),
                extent_bits: pad.to_bits(),
                spread_bits: shadow.spread.to_bits(),
                falloff_tag: layer.falloff.tag(),
            };

            if !self.patch_cache.contains_key(&key) {
                while self.patch_cache.len() >= MAX_SHADOW_PATCHES
                    || self.patch_bytes >= MAX_SHADOW_BYTES
                {
                    let before = self.patch_cache.len();
                    self.evict_oldest_patch();
                    if self.patch_cache.len() == before {
                        break;
                    }
                }
                let patch = generate_patch(
                    &mut self.scratch_a,
                    &mut self.scratch_b,
                    &mut self.scratch_temp,
                    r_scalar,
                    blur_r,
                    &layer.falloff.lut(),
                    kernel_pad,
                    corner_size,
                    patch_dim,
                    shadow.spread,
                );
                self.patch_bytes += patch.mask.len();
                self.patch_cache.insert(key, patch);
            }
            self.touch_patch(key);

            let patch = self.patch_cache.get(&key).unwrap();
            blit_9patch(
                pixmap,
                rect,
                offset_x,
                offset_y,
                pad,
                shadow.color,
                layer_opacity,
                shadow.blend,
                transform,
                patch,
                clip,
            );
        }
    }
}

impl Default for ShadowRasterizer {
    fn default() -> Self {
        Self::new()
    }
}
