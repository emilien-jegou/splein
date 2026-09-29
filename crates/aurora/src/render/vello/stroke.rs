// Single responsibility: Analytical border alignment and stroke styling for Kurbo.

use crate::foundation::{Radius, ResolvedRect, Stroke, StrokeAlign};

/// Computes the adjusted stroke geometry and clamped radius for analytical border alignment.
#[inline(always)]
pub fn adjust_stroke_geometry(rect: &ResolvedRect, radius: Radius, stroke: &Stroke) -> (ResolvedRect, Radius) {
    let half = stroke.width / 2.0;
    match stroke.align {
        StrokeAlign::Inside => {
            let adj_rect = ResolvedRect::new(
                rect.x + half, rect.y + half, (rect.width - stroke.width).max(0.0), (rect.height - stroke.width).max(0.0),
            );
            let adj_radius = match radius {
                Radius::Scalar(r) => Radius::Scalar((r - half).max(0.0)),
                Radius::Max => Radius::Max,
                Radius::Corners { top_left, top_right, bottom_right, bottom_left } => Radius::Corners {
                    top_left: (top_left - half).max(0.0),
                    top_right: (top_right - half).max(0.0),
                    bottom_right: (bottom_right - half).max(0.0),
                    bottom_left: (bottom_left - half).max(0.0),
                },
            };
            (adj_rect, adj_radius)
        }
        StrokeAlign::Outside => {
            let adj_rect = ResolvedRect::new(
                rect.x - half, rect.y - half, rect.width + stroke.width, rect.height + stroke.width,
            );
            let adj_radius = match radius {
                Radius::Scalar(r) => Radius::Scalar(r + half),
                Radius::Max => Radius::Max,
                Radius::Corners { top_left, top_right, bottom_right, bottom_left } => Radius::Corners {
                    top_left: top_left + half,
                    top_right: top_right + half,
                    bottom_right: bottom_right + half,
                    bottom_left: bottom_left + half,
                },
            };
            (adj_rect, adj_radius)
        }
        StrokeAlign::Center => (*rect, radius),
    }
}
