// crates/splein/src/domain/geometry.rs

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_squared(self, other: Self) -> f32 {
        (self.x - other.x).powi(2) + (self.y - other.y).powi(2)
    }

    pub fn distance(self, other: Self) -> f32 {
        self.distance_squared(other).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub const EMPTY: Self = Self {
        min: Vec2 { x: f32::INFINITY, y: f32::INFINITY },
        max: Vec2 { x: f32::NEG_INFINITY, y: f32::NEG_INFINITY },
    };

    pub fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn from_point(p: Vec2) -> Self {
        Self { min: p, max: p }
    }

    pub fn expand_with_point(&mut self, p: Vec2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    #[inline]
    pub fn contains_with_padding(&self, p: Vec2, padding: f32) -> bool {
        p.x >= self.min.x - padding
            && p.x <= self.max.x + padding
            && p.y >= self.min.y - padding
            && p.y <= self.max.y + padding
    }

    pub fn translate(&mut self, delta: Vec2) {
        self.min.x += delta.x;
        self.min.y += delta.y;
        self.max.x += delta.x;
        self.max.y += delta.y;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputSample {
    pub position: Vec2,
    pub pressure: f32,
}

impl InputSample {
    pub fn new(x: f32, y: f32, pressure: f32) -> Self {
        Self {
            position: Vec2::new(x, y),
            pressure: pressure.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CubicBezierSegment {
    pub start: Vec2,
    pub ctrl1: Vec2,
    pub ctrl2: Vec2,
    pub end: Vec2,
}

/// Real-time Midpoint B-Spline Engine with Degree Elevation (Apple PencilKit / Paper model)
/// Strictly O(1): exactly 1 smooth segment produced per motion event without lag or overshoots.
#[derive(Debug, Clone, Default)]
pub struct MidpointSpline {
    prev_point: Option<Vec2>,
    prev_midpoint: Option<Vec2>,
    history: Vec<InputSample>,
    live_tip: Option<(Vec2, Vec2)>,
}

impl MidpointSpline {
    pub fn new() -> Self {
        Self {
            prev_point: None,
            prev_midpoint: None,
            history: Vec::with_capacity(128),
            live_tip: None,
        }
    }

    /// O(1): Pushes sample and calculates the next C1-continuous Bézier segment
    pub fn push(&mut self, sample: InputSample) -> Option<CubicBezierSegment> {
        if let Some(last) = self.history.last() {
            // Filter jitter threshold (2.5px)
            if last.position.distance_squared(sample.position) < 6.25 {
                return None;
            }
        }

        self.history.push(sample);
        let curr = sample.position;

        let Some(prev) = self.prev_point else {
            self.prev_point = Some(curr);
            return None;
        };

        let Some(prev_mid) = self.prev_midpoint else {
            // First segment: line connecting P0 to initial midpoint (P0 + P1) / 2
            let mid0 = Vec2::new((prev.x + curr.x) / 2.0, (prev.y + curr.y) / 2.0);
            self.prev_point = Some(curr);
            self.prev_midpoint = Some(mid0);
            self.live_tip = Some((mid0, curr));

            return Some(CubicBezierSegment {
                start: prev,
                ctrl1: Vec2::new(prev.x + (mid0.x - prev.x) / 3.0, prev.y + (mid0.y - prev.y) / 3.0),
                ctrl2: Vec2::new(prev.x + 2.0 * (mid0.x - prev.x) / 3.0, prev.y + 2.0 * (mid0.y - prev.y) / 3.0),
                end: mid0,
            });
        };

        // Midpoint between P_{k-1} and P_k
        let curr_mid = Vec2::new((prev.x + curr.x) / 2.0, (prev.y + curr.y) / 2.0);

        // Degree elevation from quadratic Bézier (prev_mid, prev, curr_mid) to cubic Bézier
        let c1 = Vec2::new(
            prev_mid.x + 2.0 * (prev.x - prev_mid.x) / 3.0,
            prev_mid.y + 2.0 * (prev.y - prev_mid.y) / 3.0,
        );
        let c2 = Vec2::new(
            curr_mid.x + 2.0 * (prev.x - curr_mid.x) / 3.0,
            curr_mid.y + 2.0 * (prev.y - curr_mid.y) / 3.0,
        );

        self.prev_point = Some(curr);
        self.prev_midpoint = Some(curr_mid);
        self.live_tip = Some((curr_mid, curr));

        Some(CubicBezierSegment {
            start: prev_mid,
            ctrl1: c1,
            ctrl2: c2,
            end: curr_mid,
        })
    }

    pub fn live_tip(&self) -> Option<(Vec2, Vec2)> {
        self.live_tip
    }

    /// Closes out the final segment from last midpoint to the final release point
    pub fn finish(&mut self) -> Vec<CubicBezierSegment> {
        self.live_tip = None;
        let mut final_segments = Vec::new();

        if let (Some(prev_mid), Some(last_point)) = (self.prev_midpoint, self.prev_point) {
            if prev_mid.distance_squared(last_point) > 0.25 {
                let c1 = Vec2::new(
                    prev_mid.x + (last_point.x - prev_mid.x) / 3.0,
                    prev_mid.y + (last_point.y - prev_mid.y) / 3.0,
                );
                let c2 = Vec2::new(
                    prev_mid.x + 2.0 * (last_point.x - prev_mid.x) / 3.0,
                    prev_mid.y + 2.0 * (last_point.y - prev_mid.y) / 3.0,
                );
                final_segments.push(CubicBezierSegment {
                    start: prev_mid,
                    ctrl1: c1,
                    ctrl2: c2,
                    end: last_point,
                });
            }
        }

        final_segments
    }

    pub fn samples(&self) -> &[InputSample] {
        &self.history
    }
}

pub fn point_to_segment_distance(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    let l2 = a.distance_squared(b);
    if l2 == 0.0 {
        return point.distance(a);
    }
    let t = ((point.x - a.x) * (b.x - a.x) + (point.y - a.y) * (b.y - a.y)) / l2;
    let t = t.clamp(0.0, 1.0);
    let projection = Vec2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
    point.distance(projection)
}
