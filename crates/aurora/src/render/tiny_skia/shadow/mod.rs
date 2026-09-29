// Single responsibility: Outer and inset shadow rasterization orchestrator with LRU 9-patch caching.

pub mod direct;
pub mod patch;

use rustc_hash::FxHashMap;
use tiny_skia::{Mask, PixmapMut, Transform};

use crate::foundation::{Radius, ResolvedRect, Shadow, ShadowKind};
use direct::render_direct_blur;
use patch::{blit_9patch, generate_patch, ShadowPatch, ShadowPatchKey};

const MAX_SHADOW_PATCHES: usize = 64;

/// Software shadow rasterizer caching 9-patch blurred corner templates.
pub struct ShadowRasterizer {
    scratch_a: Vec<u8>,
    scratch_b: Vec<u8>,
    scratch_temp: Vec<u8>,
    patch_cache: FxHashMap<ShadowPatchKey, ShadowPatch>,
}

impl ShadowRasterizer {
    /// Constructs a clean shadow rasterizer.
    pub fn new() -> Self {
        Self {
            scratch_a: Vec::with_capacity(128 * 128),
            scratch_b: Vec::with_capacity(128 * 128),
            scratch_temp: Vec::with_capacity(128 * 128),
            patch_cache: FxHashMap::default(),
        }
    }

    /// Renders analytical soft drop shadow or direct blurred inset shadow.
    pub fn render(
        &mut self,
        pixmap: &mut PixmapMut,
        rect: &ResolvedRect,
        radius: Radius,
        shadow: &Shadow,
        opacity: f32,
        transform: Transform,
        clip: Option<&Mask>,
    ) {
        if shadow.blur <= 0.0 && shadow.offset_x == 0.0 && shadow.offset_y == 0.0 { return; }
        let total_opacity = opacity * shadow.color.a;
        if total_opacity <= 0.0 { return; }

        let r_scalar = match radius {
            Radius::Scalar(v) => v.max(0.0),
            Radius::Max => (rect.width.min(rect.height) * 0.5).max(0.0),
            Radius::Corners { .. } => radius.resolve(rect.width, rect.height),
        };

        let blur_r = shadow.blur.max(0.5);
        let kernel_pad = (shadow.spread + 3.0 * blur_r).ceil() as usize;
        let corner_size = (r_scalar.ceil() as usize) + kernel_pad;
        let patch_dim = corner_size * 2;

        let is_too_small = rect.width < (patch_dim as f32) || rect.height < (patch_dim as f32) || shadow.kind == ShadowKind::Inset;
        if is_too_small {
            render_direct_blur(&mut self.scratch_a, &mut self.scratch_b, &mut self.scratch_temp, pixmap, rect, r_scalar, shadow, total_opacity, transform, clip);
            return;
        }

        let key = ShadowPatchKey {
            radius_bits: r_scalar.to_bits(),
            blur_bits: blur_r.to_bits(),
            spread_bits: shadow.spread.to_bits(),
        };

        if !self.patch_cache.contains_key(&key) {
            if self.patch_cache.len() >= MAX_SHADOW_PATCHES {
                if let Some(&oldest) = self.patch_cache.keys().next() { self.patch_cache.remove(&oldest); }
            }
            let patch = generate_patch(&mut self.scratch_a, &mut self.scratch_b, &mut self.scratch_temp, r_scalar, blur_r, kernel_pad, corner_size, patch_dim);
            self.patch_cache.insert(key, patch);
        }

        let patch = self.patch_cache.get(&key).unwrap();
        blit_9patch(pixmap, rect, shadow, total_opacity, transform, patch);
    }
}

impl Default for ShadowRasterizer { fn default() -> Self { Self::new() } }
