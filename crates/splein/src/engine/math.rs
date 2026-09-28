// crates/presentify/src/engine/math.rs
use tiny_skia::{Path, PathBuilder, Point as SkPoint};

#[derive(Debug, Clone, Copy)]
pub struct InputPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

impl InputPoint {
    pub fn new(x: f32, y: f32, pressure: f32) -> Self {
        Self { x, y, pressure }
    }

    pub fn to_skia(&self) -> SkPoint {
        SkPoint::from_xy(self.x, self.y)
    }

    pub fn distance_to(&self, other: &InputPoint) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// Converts a sequence of points into a smooth cubic Bézier vector path
pub fn build_smooth_path(points: &[InputPoint]) -> Option<Path> {
    if points.len() < 2 {
        return None;
    }

    let mut builder = PathBuilder::new();
    builder.move_to(points[0].x, points[0].y);

    if points.len() == 2 {
        builder.line_to(points[1].x, points[1].y);
        return builder.finish();
    }

    // Catmull-Rom to Cubic Bézier spline interpolation
    for i in 0..points.len() - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < points.len() { points[i + 2] } else { p2 };

        // Standard Catmull-Rom tangent tension (alpha = 0.5)
        let c1x = p1.x + (p2.x - p0.x) / 6.0;
        let c1y = p1.y + (p2.y - p0.y) / 6.0;
        let c2x = p2.x - (p3.x - p1.x) / 6.0;
        let c2y = p2.y - (p3.y - p1.y) / 6.0;

        builder.cubic_to(c1x, c1y, c2x, c2y, p2.x, p2.y);
    }

    builder.finish()
}

/// Point-to-segment distance for the stroke eraser
pub fn distance_point_to_segment(p: InputPoint, a: InputPoint, b: InputPoint) -> f32 {
    let l2 = (b.x - a.x).powi(2) + (b.y - a.y).powi(2);
    if l2 == 0.0 {
        return p.distance_to(&a);
    }
    let t = ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / l2;
    let t = t.clamp(0.0, 1.0);
    let proj = InputPoint::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y), 0.0);
    p.distance_to(&proj)
}
