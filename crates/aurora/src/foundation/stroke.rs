// Single responsibility: Border outline definition and geometric stroke alignment.

use crate::foundation::color::Color;
use crate::foundation::fill::Fill;

/// Geometric alignment of a stroke outline relative to boundary edge.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

/// Border outline specification including stroke width, fill, and alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub fill: Fill,
    pub align: StrokeAlign,
}

impl Stroke {
    /// Creates a new stroke specification.
    pub fn new(width: f32, fill: Fill, align: StrokeAlign) -> Self {
        Self { width, fill, align }
    }

    /// Constructs an interior stroke with a solid color.
    pub fn inside(width: f32, color: Color) -> Self {
        Self { width, fill: Fill::solid(color), align: StrokeAlign::Inside }
    }

    /// Constructs a centered border stroke with a solid color.
    pub fn center(width: f32, color: Color) -> Self {
        Self { width, fill: Fill::solid(color), align: StrokeAlign::Center }
    }

    /// Constructs an exterior stroke with a solid color.
    pub fn outside(width: f32, color: Color) -> Self {
        Self { width, fill: Fill::solid(color), align: StrokeAlign::Outside }
    }
}
