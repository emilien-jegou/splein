// Single responsibility: Layered elevation-based shadow rasterization with LRU 9-patch caching.

pub mod alpha;
pub mod direct;
pub mod patch;

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tiny_skia::{Mask, Pixmap, PixmapMut, Transform};

use riode::foundation::{LightEnvironment, Radius, ResolvedRect, Shadow};
use direct::{render_direct_blur, render_inset};
use patch::{blit_9patch, generate_patch, layer_geometry, ShadowPatch, ShadowPatchKey};

const MAX_SHADOW_PATCHES: usize = 64;

/// Byte budget for patch masks: count caps alone admit 64 adversarial
/// patches (~2 MB each at huge softness → ~128 MB). Typical entries are
/// ~10 KB; oversize singles bypass like everywhere else.
const MAX_SHADOW_BYTES: usize = 8 << 20;

/// Count cap for direct-path blurred coverage (same order as patches).
const MAX_DIRECT_CACHED: usize = 64;

/// Byte budget for direct coverage (whole-rect footprints dwarf patches).
const MAX_DIRECT_BYTES: usize = 8 << 20;

/// Identity of one direct-blurred coverage bitmap: geometry only (no
/// color/opacity/transform/clip — those apply at tint/draw time, so one
/// entry serves every paint of the same shape). Offset IS included: it
/// shifts the shape within same-size buffers. `inset` separates the
/// inverted (interior-clipped) coverage from the drop-path coverage —
/// identical geometry can produce both, and they are not interchangeable.
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) struct DirectKey {
    blur_bits: u32,
    spread_bits: u32,
    falloff_tag: u8,
    w_bits: u32,
    h_bits: u32,
    radius_bits: u32,
    offset_bits: [u32; 2],
    inset: u8,
}

/// Software shadow rasterizer caching 9-patch blurred corner templates.
///
/// Renders each [`Shadow`] layer back-to-front (largest extent first) with
/// its own falloff LUT, compositing with the shadow's blend mode.

impl Default for ShadowRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

pub mod bridge;
pub use bridge::*;
pub use riode_shader::shadow::*;
pub use riode_shader::shadow_ext::*;

pub use riode_shader::api as shadow_api;

// Re-export from riode-shader for compatibility
pub use riode_shader::shadow::ShadowRasterizer;
