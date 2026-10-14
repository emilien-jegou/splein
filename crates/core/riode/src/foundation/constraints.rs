// Single responsibility: Measurement constraints, definiteness checks, and panic-safe clamping.

/// Two-dimensional bounding box constraints for layout measurement passes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Constraints {
    /// Lower horizontal dimension bound in pixels.
    pub min_width: f32,
    /// Upper horizontal dimension bound in pixels.
    pub max_width: f32,
    /// Lower vertical dimension bound in pixels.
    pub min_height: f32,
    /// Upper vertical dimension bound in pixels.
    pub max_height: f32,
}

impl Constraints {
    /// Completely unbounded measurement constraints.
    pub const UNCONSTRAINED: Self = Self {
        min_width: 0.0,
        max_width: f32::INFINITY,
        min_height: 0.0,
        max_height: f32::INFINITY,
    };

    /// Loose constraints with bounded maxima and zero minima.
    pub fn loose(max_width: f32, max_height: f32) -> Self {
        Self {
            min_width: 0.0,
            max_width: if max_width.is_finite() {
                max_width.max(0.0)
            } else {
                f32::INFINITY
            },
            min_height: 0.0,
            max_height: if max_height.is_finite() {
                max_height.max(0.0)
            } else {
                f32::INFINITY
            },
        }
    }

    /// Tight constraints enforcing exact width and height dimensions.
    pub fn tight(width: f32, height: f32) -> Self {
        let w = if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        };
        let h = if height.is_finite() {
            height.max(0.0)
        } else {
            0.0
        };
        Self {
            min_width: w,
            max_width: w,
            min_height: h,
            max_height: h,
        }
    }

    /// Exact width with unbounded height, for measuring main size at a known cross size.
    pub fn tight_width(width: f32) -> Self {
        let w = if width.is_finite() {
            width.max(0.0)
        } else {
            f32::INFINITY
        };
        Self {
            min_width: w,
            max_width: w,
            min_height: 0.0,
            max_height: f32::INFINITY,
        }
    }

    /// Whether width is constrained to a single exact dimension.
    pub fn is_tight_width(&self) -> bool {
        self.min_width >= self.max_width
    }

    /// Whether height is constrained to a single exact dimension.
    pub fn is_tight_height(&self) -> bool {
        self.min_height >= self.max_height
    }

    /// Clamps horizontal dimension within bounds without panic on inverted bounds or NaN.
    pub fn clamp_width(&self, w: f32) -> f32 {
        let min = if self.min_width.is_finite() {
            self.min_width.max(0.0)
        } else {
            0.0
        };
        let max = if self.max_width.is_finite() {
            self.max_width.max(min)
        } else {
            f32::INFINITY
        };
        let val = if w.is_finite() { w } else { min };
        val.clamp(min, max)
    }

    /// Clamps vertical dimension within bounds without panic on inverted bounds or NaN.
    pub fn clamp_height(&self, h: f32) -> f32 {
        let min = if self.min_height.is_finite() {
            self.min_height.max(0.0)
        } else {
            0.0
        };
        let max = if self.max_height.is_finite() {
            self.max_height.max(min)
        } else {
            f32::INFINITY
        };
        let val = if h.is_finite() { h } else { min };
        val.clamp(min, max)
    }
}

/// Natural bounding size calculated by content without parent expansion.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct IntrinsicSize {
    /// Intrinsic horizontal dimension.
    pub width: f32,
    /// Intrinsic vertical dimension.
    pub height: f32,
}

/// Intermediate desired size requested by a layout node.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct DesiredSize {
    /// Desired horizontal dimension.
    pub width: f32,
    /// Desired vertical dimension.
    pub height: f32,
}
