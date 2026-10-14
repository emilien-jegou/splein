// Single responsibility: TinySkia path generation for W3C-compliant clamped rounded rectangles.

use riode::foundation::{Radius, ResolvedRect};
use tiny_skia::{Path, PathBuilder, Rect, Transform};

/// Snaps a rect's placement to device pixels under translation-only
/// transforms: shifts the rect so its effective origin lands on integer
/// pixels. Uses floor (never round): round-half-away-from-zero maps the
/// same .5 fraction to different shifts depending on the accumulated
/// translation's sign (e.g. -27.5 -> -28 but +1.5 -> +2), which splits
/// coincident edges — the white-box bottom and the selection-highlight
/// bottom share device y=263.5 through different offset chains, and opposite
/// shifts opened exactly the 1px seam (highlight over green, box missing).
/// Floor shifts equal fractions equally, so coincident edges stay
/// coincident. Scaled/skewed transforms pass through untouched (fractional
/// coverage is correct there, and motion smoothness outranks static
/// crispness under animation).
pub fn snap_rect_to_device(rect: &ResolvedRect, tx: Transform) -> ResolvedRect {
    if tx.has_skew() || tx.has_scale() {
        return *rect;
    }
    let dx = tx.tx.floor() - tx.tx;
    let dy = tx.ty.floor() - tx.ty;
    if dx == 0.0 && dy == 0.0 {
        return *rect;
    }
    ResolvedRect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

/// Snaps the shared translation itself to device pixels: every rect drawn
/// under the returned transform shifts by the SAME amount, so coincident
/// edges (selection highlight == text-box bottom == clip edge) stay
/// coincident and land integer together. Per-rect snapping cannot do this:
/// each rect would round a different fraction and split the shared edge
/// into a 1px gap. Floor (not round) for the same sign-independence reason
/// as above. Scaled/skewed transforms pass through untouched.
pub fn snap_transform_to_device(mut tx: Transform) -> Transform {
    if tx.has_skew() || tx.has_scale() {
        return tx;
    }
    tx.tx = tx.tx.floor();
    tx.ty = tx.ty.floor();
    tx
}

/// Builds rounded rectangle path with strict W3C Bézier radius clamping supporting per-corner radii.
pub fn build_rounded_path(rect: &ResolvedRect, radius: Radius) -> Path {
    let mut builder = PathBuilder::new();
    let c = radius.resolve_corners(rect.width, rect.height);

    if c.top_left <= 0.0 && c.top_right <= 0.0 && c.bottom_right <= 0.0 && c.bottom_left <= 0.0 {
        if let Some(skia_rect) = Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            return PathBuilder::from_rect(skia_rect);
        }
        return builder
            .finish()
            .unwrap_or_else(|| PathBuilder::new().finish().unwrap());
    }

    let kappa = 0.552_284_8;
    let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);

    builder.move_to(x + c.top_left, y);
    builder.line_to(x + w - c.top_right, y);
    if c.top_right > 0.0 {
        let k = c.top_right * (1.0 - kappa);
        builder.cubic_to(x + w - k, y, x + w, y + k, x + w, y + c.top_right);
    }

    builder.line_to(x + w, y + h - c.bottom_right);
    if c.bottom_right > 0.0 {
        let k = c.bottom_right * (1.0 - kappa);
        builder.cubic_to(
            x + w,
            y + h - k,
            x + w - k,
            y + h,
            x + w - c.bottom_right,
            y + h,
        );
    }

    builder.line_to(x + c.bottom_left, y + h);
    if c.bottom_left > 0.0 {
        let k = c.bottom_left * (1.0 - kappa);
        builder.cubic_to(x + k, y + h, x, y + h - k, x, y + h - c.bottom_left);
    }

    builder.line_to(x, y + c.top_left);
    if c.top_left > 0.0 {
        let k = c.top_left * (1.0 - kappa);
        builder.cubic_to(x, y + k, x + k, y, x + c.top_left, y);
    }
    builder.close();

    builder
        .finish()
        .unwrap_or_else(|| PathBuilder::new().finish().unwrap())
}
