// Single responsibility: W3C CSS-compliant corner radius specification and boundary resolution.

/// Corner radius specification supporting uniform, pill, and per-corner definitions.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Radius {
    /// Uniform scalar corner radius across all four corners.
    Scalar(f32),
    /// Automatic pill rounding based on the smaller dimension.
    Max,
    /// Explicit independent radii for top-left, top-right, bottom-right, and bottom-left.
    Corners {
        /// Top-left corner radius.
        top_left: f32,
        /// Top-right corner radius.
        top_right: f32,
        /// Bottom-right corner radius.
        bottom_right: f32,
        /// Bottom-left corner radius.
        bottom_left: f32,
    },
}

/// Resolved physical pixel radii for all four corners after W3C clamping.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct ResolvedCorners {
    /// Clamped top-left radius.
    pub top_left: f32,
    /// Clamped top-right radius.
    pub top_right: f32,
    /// Clamped bottom-right radius.
    pub bottom_right: f32,
    /// Clamped bottom-left radius.
    pub bottom_left: f32,
}

impl Radius {
    /// Zero corner radius.
    pub const ZERO: Self = Self::Scalar(0.0);

    /// Constructs a uniform scalar corner radius.
    pub const fn scalar(value: f32) -> Self { Self::Scalar(value) }

    /// Constructs a full pill radius.
    pub const fn max() -> Self { Self::Max }

    /// Constructs an explicit 4-corner radius.
    pub const fn corners(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self::Corners { top_left, top_right, bottom_right, bottom_left }
    }

    /// Constructs a radius affecting only the top two corners.
    pub const fn top(r: f32) -> Self { Self::Corners { top_left: r, top_right: r, bottom_right: 0.0, bottom_left: 0.0 } }

    /// Constructs a radius affecting only the bottom two corners.
    pub const fn bottom(r: f32) -> Self { Self::Corners { top_left: 0.0, top_right: 0.0, bottom_right: r, bottom_left: r } }

    /// Resolves corners into physical pixels with W3C CSS proportional scaling if overlapping.
    pub fn resolve_corners(&self, width: f32, height: f32) -> ResolvedCorners {
        let (w, h) = (width.max(0.0), height.max(0.0));
        let (tl, tr, br, bl) = match *self {
            Self::Scalar(r) => (r.max(0.0), r.max(0.0), r.max(0.0), r.max(0.0)),
            Self::Max => {
                let m = (w.min(h) * 0.5).max(0.0);
                (m, m, m, m)
            }
            Self::Corners { top_left, top_right, bottom_right, bottom_left } => (
                top_left.max(0.0), top_right.max(0.0), bottom_right.max(0.0), bottom_left.max(0.0)
            ),
        };

        let top_sum = tl + tr;
        let bottom_sum = bl + br;
        let left_sum = tl + bl;
        let right_sum = tr + br;

        let mut factor = 1.0f32;
        if top_sum > w && top_sum > 0.0 { factor = factor.min(w / top_sum); }
        if bottom_sum > w && bottom_sum > 0.0 { factor = factor.min(w / bottom_sum); }
        if left_sum > h && left_sum > 0.0 { factor = factor.min(h / left_sum); }
        if right_sum > h && right_sum > 0.0 { factor = factor.min(h / right_sum); }

        ResolvedCorners {
            top_left: tl * factor,
            top_right: tr * factor,
            bottom_right: br * factor,
            bottom_left: bl * factor,
        }
    }

    /// Resolves an average scalar radius for backwards compatibility.
    pub fn resolve(&self, width: f32, height: f32) -> f32 {
        let c = self.resolve_corners(width, height);
        (c.top_left + c.top_right + c.bottom_right + c.bottom_left) * 0.25
    }
}

impl Default for Radius { fn default() -> Self { Self::ZERO } }
impl From<f32> for Radius { fn from(r: f32) -> Self { Self::Scalar(r) } }
