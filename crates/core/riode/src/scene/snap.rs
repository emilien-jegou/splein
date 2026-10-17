// Single responsibility: Shared snap/rounding/quantization policies for all backends.
//
// These are *placement decisions* (which pixel a mark lands on), not pixel
// filling — so they live in core, parameterized by the device inputs each
// engine supplies (scale, canvas translation). Engines call these instead of
// spelling the policy out per backend; the policies can no longer diverge.
//
// Rounding convention: `floor`, never `round`, for translation snaps.
// Round-half-away-from-zero maps the same .5 fraction to different shifts
// depending on the accumulated translation's sign (e.g. -27.5 -> -28 but
// +1.5 -> +2), which splits coincident edges — the white-box bottom and the
// selection-highlight bottom share a device y through different offset
// chains, and opposite shifts opened exactly a 1px seam. Floor shifts equal
// fractions equally, so coincident edges stay coincident.

use crate::foundation::{ResolvedRect, Transform};

/// Whether the transform carries scale or skew: snapped placement is only
/// valid for translation-only transforms (fractional coverage is correct
/// under scale/skew, and motion smoothness outranks static crispness there).
#[inline(always)]
pub fn has_scale_or_skew(t: &Transform) -> bool {
    t.a != 1.0 || t.d != 1.0 || t.b != 0.0 || t.c != 0.0
}

/// Snaps a translation to device pixels (floor). Every rect drawn under the
/// returned transform shifts by the SAME amount, so coincident edges stay
/// coincident and land integer together.
#[inline(always)]
pub fn snap_transform_to_device(t: &Transform) -> Transform {
    if has_scale_or_skew(t) {
        return *t;
    }
    Transform {
        tx: t.tx.floor(),
        ty: t.ty.floor(),
        ..*t
    }
}

/// Snaps a rect's placement to device pixels under translation-only
/// transforms: shifts the rect so its effective origin lands on integer
/// pixels. Scaled/skewed transforms pass through untouched.
#[inline(always)]
pub fn snap_rect_to_device(rect: &ResolvedRect, t: &Transform) -> ResolvedRect {
    if has_scale_or_skew(t) {
        return *rect;
    }
    let dx = t.tx.floor() - t.tx;
    let dy = t.ty.floor() - t.ty;
    if dx == 0.0 && dy == 0.0 {
        return *rect;
    }
    ResolvedRect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

/// Hairline rule: thin rects (≤1.5px) at fractional positions split coverage
/// over two rows (full-width faint double line). Snaps thin
/// translation-only rects to integer pixels so they stay crisp.
#[inline(always)]
pub fn hairline_adjust(x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
    let (mut rx, mut ry, mut rw, mut rh) = (x, y, w, h);
    if rh <= 1.5 {
        ry = ry.round();
        rh = rh.round().max(1.0);
    }
    if rw <= 1.5 {
        rx = rx.round();
        rw = rw.round().max(1.0);
    }
    (rx, ry, rw, rh)
}

/// Splits a device position into `(integer base, subpixel quantum)` over
/// `quanta` bins, wrapping `q == quanta` into `base + 1`.
#[inline(always)]
pub fn quantize_nths(pos: f32, quanta: f32) -> (i32, u8) {
    let base = pos.floor();
    let mut q = ((pos - base) * quanta).round() as i32;
    let mut b = base as i32;
    if q >= quanta as i32 {
        b += 1;
        q = 0;
    }
    (b, q.max(0) as u8)
}

/// X quantum: quarters (≤1/8px snap).
#[inline(always)]
pub fn quantize_quarter(pos: f32) -> (i32, u8) {
    quantize_nths(pos, 4.0)
}

/// Y quantum: eighths (≤1/16px snap, so horizontal features never read as
/// doubled rows).
#[inline(always)]
pub fn quantize_eighth(pos: f32) -> (i32, u8) {
    quantize_nths(pos, 8.0)
}

/// Raster size for a size-keyed cache entry (SVG, damage tiles): drawn
/// device size, so static content re-rasters only when its drawn size
/// changes.
#[inline(always)]
pub fn raster_cache_size(w: f32, h: f32, scale: f32) -> (u32, u32) {
    ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32)
}

/// Expands a rect outward to integer pixels (conservative for damage/cull).
#[inline(always)]
pub fn integerize_damage(rect: &ResolvedRect) -> ResolvedRect {
    let x0 = rect.x.floor();
    let y0 = rect.y.floor();
    ResolvedRect::new(
        x0,
        y0,
        (rect.right().ceil() - x0).max(0.0),
        (rect.bottom().ceil() - y0).max(0.0),
    )
}

/// Settle rounding for a resting text run: nearest whole pixel (max 0.5px
/// nudge, least-visible snap).
#[inline(always)]
pub fn settle_round(y: f32) -> f32 {
    y.round()
}
