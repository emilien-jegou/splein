// Single responsibility: TinySkia path generation for W3C-compliant clamped rounded rectangles.

use aurora::foundation::{Radius, ResolvedRect};
use tiny_skia::{Path, PathBuilder, Rect, Transform};

/// Snaps a rect's placement to device pixels under translation-only
/// transforms: shifts the rect by the fractional part of the transform
/// translation so effective edges land on integer pixels and symmetric AA
/// replaces one-sided gray fringes. Scaled/skewed transforms and sizes pass
/// through untouched (fractional coverage is correct there, and motion
/// smoothness outranks static crispness under animation).
pub fn snap_rect_to_device(rect: &ResolvedRect, tx: Transform) -> ResolvedRect {
    if tx.has_skew() || tx.has_scale() {
        return *rect;
    }
    let dx = tx.tx.round() - tx.tx;
    let dy = tx.ty.round() - tx.ty;
    if dx == 0.0 && dy == 0.0 {
        return *rect;
    }
    ResolvedRect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
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
