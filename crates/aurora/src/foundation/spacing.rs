// Single responsibility: External margin, internal padding, and inter-element gap spatial models.

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

    /// Field-wise merge for chained `.margin()` builders.
    ///
    /// Nonzero sides of `over` win; zero sides keep `self`. This is what lets
    /// `.margin(Margin::left(300.0)).margin(Margin::top(200.0))` compose instead
    /// of the second call silently discarding the first: the per-side
    /// constructors only make sense when they accumulate.
    pub const fn merged(self, over: Self) -> Self {
        Self {
            top: if over.top != 0.0 { over.top } else { self.top },
            right: if over.right != 0.0 {
                over.right
            } else {
                self.right
            },
            bottom: if over.bottom != 0.0 {
                over.bottom
            } else {
                self.bottom
            },
            left: if over.left != 0.0 {
                over.left
            } else {
                self.left
            },
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

/// Internal spacing inset applied inside an element's bounding box.
///
/// Padding shrinks the content box its children lay out in and grows the
/// element's intrinsic size; unlike [`Margin`] it never offsets the element
/// within its parent. Paint (fill/stroke) still covers the full box.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
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

    /// Applies a top padding only.
    pub const fn top(val: f32) -> Self {
        Self {
            top: val,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
    /// Applies a right padding only.
    pub const fn right(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: 0.0,
        }
    }
    /// Applies a bottom padding only.
    pub const fn bottom(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: val,
            left: 0.0,
        }
    }
    /// Applies a left padding only.
    pub const fn left(val: f32) -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: val,
        }
    }

    /// Applies a symmetric horizontal (left and right) padding.
    pub const fn x(val: f32) -> Self {
        Self {
            top: 0.0,
            right: val,
            bottom: 0.0,
            left: val,
        }
    }
    /// Applies a symmetric vertical (top and bottom) padding.
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

    /// Field-wise merge for chained `.padding()` builders.
    ///
    /// Nonzero sides of `over` win; zero sides keep `self`. This is what lets
    /// `.padding(Padding::left(8.0)).padding(Padding::top(4.0))` compose instead
    /// of the second call silently discarding the first.
    pub const fn merged(self, over: Self) -> Self {
        Self {
            top: if over.top != 0.0 { over.top } else { self.top },
            right: if over.right != 0.0 {
                over.right
            } else {
                self.right
            },
            bottom: if over.bottom != 0.0 {
                over.bottom
            } else {
                self.bottom
            },
            left: if over.left != 0.0 {
                over.left
            } else {
                self.left
            },
        }
    }

    /// Returns the combined left and right padding.
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    /// Returns the combined top and bottom padding.
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

impl From<Margin> for Padding {
    fn from(m: Margin) -> Self {
        Self {
            top: m.top,
            right: m.right,
            bottom: m.bottom,
            left: m.left,
        }
    }
}

impl From<f32> for Padding {
    fn from(val: f32) -> Self {
        Self::all(val)
    }
}

impl From<i32> for Padding {
    fn from(val: i32) -> Self {
        Self::all(val as f32)
    }
}

impl From<u32> for Padding {
    fn from(val: u32) -> Self {
        Self::all(val as f32)
    }
}

/// Spacing interval applied between adjacent layout siblings.
///
/// `Fixed(px)` applies one interval on both axes. `Split` separates the
/// main-axis interval (between items on one line) from the cross-axis
/// interval (between wrapped lines): `row_gap` sets cross, `col_gap` sets
/// main. For a horizontal container that reads naturally (columns apart vs
/// rows apart); for vertical containers the axes transpose — documented at
/// the builder.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Gap {
    Fixed(f32),
    Split { main: f32, cross: f32 },
}

impl Gap {
    /// Interval between items along the main axis (within one line).
    pub fn main_px(self) -> f32 {
        match self {
            Gap::Fixed(px) if px.is_finite() => px.max(0.0),
            Gap::Fixed(_) => 0.0,
            Gap::Split { main, .. } if main.is_finite() => main.max(0.0),
            Gap::Split { .. } => 0.0,
        }
    }

    /// Interval between wrapped lines along the cross axis.
    pub fn cross_px(self) -> f32 {
        match self {
            Gap::Fixed(px) if px.is_finite() => px.max(0.0),
            Gap::Fixed(_) => 0.0,
            Gap::Split { cross, .. } if cross.is_finite() => cross.max(0.0),
            Gap::Split { .. } => 0.0,
        }
    }

    /// Returns a copy with the main-axis interval replaced.
    pub fn with_main(self, main: f32) -> Self {
        Gap::Split {
            main,
            cross: self.cross_px(),
        }
    }

    /// Returns a copy with the cross-axis (inter-line) interval replaced.
    pub fn with_cross(self, cross: f32) -> Self {
        Gap::Split {
            main: self.main_px(),
            cross,
        }
    }
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
