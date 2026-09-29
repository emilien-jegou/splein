// Single responsibility: TinySkia path generation for W3C-compliant clamped rounded rectangles.

use tiny_skia::{Path, PathBuilder, Rect};
use crate::foundation::{Radius, ResolvedRect};

/// Builds rounded rectangle path with strict W3C Bézier radius clamping supporting per-corner radii.
pub fn build_rounded_path(rect: &ResolvedRect, radius: Radius) -> Path {
    let mut builder = PathBuilder::new();
    let c = radius.resolve_corners(rect.width, rect.height);

    if c.top_left <= 0.0 && c.top_right <= 0.0 && c.bottom_right <= 0.0 && c.bottom_left <= 0.0 {
        if let Some(skia_rect) = Rect::from_xywh(rect.x, rect.y, rect.width, rect.height) {
            return PathBuilder::from_rect(skia_rect);
        }
        return builder.finish().unwrap_or_else(|| PathBuilder::new().finish().unwrap());
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
        builder.cubic_to(x + w, y + h - k, x + w - k, y + h, x + w - c.bottom_right, y + h);
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

    builder.finish().unwrap_or_else(|| PathBuilder::new().finish().unwrap())
}
