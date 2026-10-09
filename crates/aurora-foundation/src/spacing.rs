// Single responsibility: External margin and inter-element gap spatial models.

/// External spacing offset applied around element bounding boxes.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Margin {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margin {
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
    pub const fn all(val: f32) -> Self {
        Self {
            top: val,
            right: val,
            bottom: val,
            left: val,
        }
    }
    pub const fn symmetric(y: f32, x: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    /// Applies a top margin only.
    pub const fn top(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
    /// Applies a right margin only.
    pub const fn right(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: 0.0,
        }
    }
    /// Applies a bottom margin only.
    pub const fn bottom(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }
    /// Applies a left margin only.
    pub const fn left(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: val,
        }
    }

    /// Applies a symmetric horizontal (left and right) margin.
    pub const fn x(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: val,
        }
    }
    /// Applies a symmetric vertical (top and bottom) margin.
    pub const fn y(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }

    /// Applies every side explicitly, clockwise from top.
    pub const fn sides(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Returns the combined left and right margin.
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    /// Returns the combined top and bottom margin.
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Spacing interval applied between adjacent layout siblings.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Gap {
    Fixed(f32),
    Full,
}

impl Default for Gap {
    fn default() -> Self {
        Self::Fixed(0.0)
    }
}

impl From<f32> for Gap {
    fn from(val: f32) -> Self {
        Self::Fixed(val)
    }
}
