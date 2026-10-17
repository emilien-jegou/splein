// Single responsibility: CPU element-filter passes for filter layers —
// premultiplied-RGBA box blur and 4x5 color matrix, applied to an offscreen.

use riode::foundation::{Filter, FilterOp, LightEnvironment, Shadow, Transform};
use riode::render::blur::box_blur_alpha;
use tiny_skia::Pixmap;

/// One alpha-silhouette shadow waiting on its layer's content: the
/// `DrawShadow` that arrived *inside* this layer (emission order:
/// BeginLayer → shadow → fill) is stashed instead of painted, so
/// `end_layer` can cast it from the finished offscreen's alpha onto the
/// parent, behind the content composite.
pub struct AlphaShadow {
    pub shadow: Shadow,
    pub light: LightEnvironment,
    pub opacity: f32,
    pub transform: Transform,
}

/// One open filter layer: an offscreen the sink redirects draws into,
/// plus what `end_layer` needs to filter and composite it.
pub struct LayerFrame {
    /// Offscreen content. `None` marks a plain clip layer (the pre-filter
    /// `has_layer` behavior): no isolation, just a clip.
    pub pixmap: Option<Pixmap>,
    /// Device-space (tile-space) origin of the offscreen's (0,0). Content
    /// transforms are shifted by `-origin` so tile coords land in the pixmap.
    pub origin: (f32, f32),
    /// Element filter to apply at `end_layer`. `Some` iff `pixmap` is `Some`.
    pub filter: Option<Filter>,
    /// Clip-stack depth at layer entry. In-layer clips sit at or above this;
    /// masks below are tile-sized and must not intersect the offscreen's.
    pub clip_floor: usize,
    /// Stashed alpha drop shadow for this layer's own node, painted at
    /// `end_layer` from the offscreen's alpha (behind the content).
    pub alpha_shadow: Option<AlphaShadow>,
}

/// Applies the filter chain to a rendered offscreen: spatial blur first at
/// the chain's largest sigma, then one combined color matrix.
///
/// Blur-before-color is the v1 ordering (documented on [`Filter`]); interleaved
/// blur/color chains are approximated by that order.
pub fn apply_filter(pixmap: &mut Pixmap, filter: &Filter) {
    let sigma = filter.max_blur_sigma();
    if sigma > 0.0 {
        // Two box iterations approximate a Gaussian with sigma ≈ 1.15·r.
        let r = ((sigma / 1.15).round() as usize).max(1);
        blur_premul_rgba(pixmap, r);
    }
    if let Some(m) = filter.color_matrix() {
        apply_color_matrix(pixmap, &m);
    }
}

/// Two-pass box blur over premultiplied RGBA, one channel plane at a time.
///
/// Blurring premultiplied channels independently is the standard approximation
/// for Gaussian blurs of alpha-composited content: it avoids the dark halo
/// that blurring straight alpha would produce at edges.
fn blur_premul_rgba(pixmap: &mut Pixmap, radius: usize) {
    let w = pixmap.width() as usize;
    let h = pixmap.height() as usize;
    if w == 0 || h == 0 || radius == 0 {
        return;
    }
    let len = w * h;
    // Pooled scratch planes: reused across blur calls on this thread so
    // per-frame blurs don't re-alloc 3×w*h each time.
    thread_local! {
        static SCRATCH: std::cell::RefCell<(Vec<u8>, Vec<u8>, Vec<u8>)> =
            std::cell::RefCell::new((Vec::new(), Vec::new(), Vec::new()));
    }
    SCRATCH.with(|cell| {
        let (src, dst, tmp) = &mut *cell.borrow_mut();
        if src.len() < len {
            src.resize(len, 0);
        }
        if dst.len() < len {
            dst.resize(len, 0);
        }
        if tmp.len() < len {
            tmp.resize(len, 0);
        }
        let src: &mut [u8] = &mut **src;
        let dst: &mut [u8] = &mut **dst;
        let tmp: &mut [u8] = &mut **tmp;
        let data = pixmap.data_mut();
    for c in 0..4 {
        for (i, px) in data.chunks_exact(4).enumerate() {
            src[i] = px[c];
        }
        // Two passes: box → box ≈ Gaussian (same iteration count the shadow
        // path uses; see box_blur_alpha_with_falloff).
        box_blur_alpha(&*src, &mut *dst, &mut *tmp, w, h, radius);
        box_blur_alpha(&*dst, &mut *src, &mut *tmp, w, h, radius);
        for (i, px) in data.chunks_exact_mut(4).enumerate() {
            px[c] = src[i];
        }
        }
    });
}

/// Applies a row-major 4x5 color matrix to premultiplied pixels.
///
/// CSS filter matrices operate on straight (non-premultiplied) RGBA, so each
/// pixel is unpremultiplied, transformed, clamped, and re-premultiplied.
fn apply_color_matrix(pixmap: &mut Pixmap, m: &[f32; 20]) {
    for px in pixmap.pixels_mut() {
        let a = px.alpha() as f32 / 255.0;
        if a <= 0.0 {
            // Transparent pixel: color is undefined under premultiplication.
            // The matrix can still move alpha (contrast's offset does not,
            // but hue/brightness rows keep alpha); recompute alpha only.
            let na = (m[15] * 0.0 + m[16] * 0.0 + m[17] * 0.0 + m[18] * 0.0 + m[19]).clamp(0.0, 1.0);
            *px = tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, (na * 255.0) as u8)
                .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
            continue;
        }
        let r = px.red() as f32 / 255.0 / a;
        let g = px.green() as f32 / 255.0 / a;
        let b = px.blue() as f32 / 255.0 / a;
        let nr = (m[0] * r + m[1] * g + m[2] * b + m[3] * a + m[4]).clamp(0.0, 1.0);
        let ng = (m[5] * r + m[6] * g + m[7] * b + m[8] * a + m[9]).clamp(0.0, 1.0);
        let nb = (m[10] * r + m[11] * g + m[12] * b + m[13] * a + m[14]).clamp(0.0, 1.0);
        let na = (m[15] * r + m[16] * g + m[17] * b + m[18] * a + m[19]).clamp(0.0, 1.0);
        // Premultiplied components are the straight color scaled by alpha, so
        // each is <= alpha by construction; from_rgba's invariant holds.
        *px = tiny_skia::PremultipliedColorU8::from_rgba(
            (nr * na * 255.0) as u8,
            (ng * na * 255.0) as u8,
            (nb * na * 255.0) as u8,
            (na * 255.0) as u8,
        )
        .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
}

/// Whether the chain contains any op this CPU path can apply.
///
/// A blur-only or color-only chain is fine; an empty chain still needs the
/// isolation (opacity-style compositing) the layer provides.
pub fn chain_applicable(filter: &Filter) -> bool {
    filter
        .ops()
        .iter()
        .all(|op| matches!(op, FilterOp::Blur(_) | FilterOp::Brightness(_) | FilterOp::Saturate(_) | FilterOp::Contrast(_) | FilterOp::HueRotate(_)))
}
