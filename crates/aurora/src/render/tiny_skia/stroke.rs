// Single responsibility: Analytical border outline alignment and rasterization.

use tiny_skia::{Mask, PixmapMut, Stroke as SkiaStroke, Transform};
use crate::foundation::{Radius, ResolvedRect, Stroke, StrokeAlign};
use crate::render::tiny_skia::path::build_rounded_path;
use crate::render::tiny_skia::shader::build_paint;

/// Renders analytical inside, outside, or centered borders without anti-aliasing seams.
pub fn render_stroke(
    pixmap: &mut PixmapMut,
    rect: &ResolvedRect,
    radius: Radius,
    stroke: &Stroke,
    opacity: f32,
    transform: Transform,
    clip: Option<&Mask>,
) {
    if stroke.width <= 0.0 || !stroke.width.is_finite() { return; }
    if !rect.width.is_finite() || !rect.height.is_finite() || rect.width <= 0.0 || rect.height <= 0.0 { return; }

    let safe_opacity = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 0.0 };
    let paint = match build_paint(&stroke.fill, safe_opacity) {
        Some(p) => p,
        None => return,
    };

    let mut skia_stroke = SkiaStroke::default();
    skia_stroke.width = stroke.width;

    let adjusted_rect = match stroke.align {
        StrokeAlign::Inside => {
            let half = stroke.width / 2.0;
            ResolvedRect::new(rect.x + half, rect.y + half, (rect.width - stroke.width).max(0.0), (rect.height - stroke.width).max(0.0))
        }
        StrokeAlign::Outside => {
            let half = stroke.width / 2.0;
            ResolvedRect::new(rect.x - half, rect.y - half, rect.width + stroke.width, rect.height + stroke.width)
        }
        StrokeAlign::Center => *rect,
    };

    let path = build_rounded_path(&adjusted_rect, radius);
    pixmap.stroke_path(&path, &paint, &skia_stroke, transform, clip);
}
