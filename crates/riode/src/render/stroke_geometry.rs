// Single responsibility: Analytical stroke alignment geometry shared by render backends.

use crate::foundation::{Radius, ResolvedRect, Stroke, StrokeAlign};

/// Adjusts the rect and corner radius for inside, center, or outside stroke alignment.
#[inline(always)]
pub fn adjust_stroke_geometry(
    rect: &ResolvedRect,
    radius: Radius,
    stroke: &Stroke,
) -> (ResolvedRect, Radius) {
    let half = stroke.width / 2.0;
    match stroke.align {
        StrokeAlign::Inside => (
            ResolvedRect::new(
                rect.x + half,
                rect.y + half,
                (rect.width - stroke.width).max(0.0),
                (rect.height - stroke.width).max(0.0),
            ),
            shrink_radius(radius, half),
        ),
        StrokeAlign::Outside => (
            ResolvedRect::new(
                rect.x - half,
                rect.y - half,
                rect.width + stroke.width,
                rect.height + stroke.width,
            ),
            grow_radius(radius, half),
        ),
        StrokeAlign::Center => (*rect, radius),
    }
}

/// Shrinks a corner radius by `half`, clamping negative values to zero.
fn shrink_radius(radius: Radius, half: f32) -> Radius {
    map_radius(radius, |r| (r - half).max(0.0))
}

/// Grows a corner radius by `half`.
fn grow_radius(radius: Radius, half: f32) -> Radius {
    map_radius(radius, |r| r + half)
}

/// Applies a scalar transform to each resolved corner radius.
fn map_radius(radius: Radius, f: impl Fn(f32) -> f32) -> Radius {
    match radius {
        Radius::Scalar(r) => Radius::Scalar(f(r)),
        Radius::Max => Radius::Max,
        Radius::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => Radius::Corners {
            top_left: f(top_left),
            top_right: f(top_right),
            bottom_right: f(bottom_right),
            bottom_left: f(bottom_left),
        },
    }
}
