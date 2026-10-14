// Visual surface fill definitions (solid, linear, radial).

use crate::foundation::color::Color;
use crate::foundation::geometry::Point;

#[derive(Clone, Debug, PartialEq)]
pub struct ColorStop {
    pub position: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: Vec<ColorStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: f32,
    pub stops: Vec<ColorStop>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}

impl Fill {
    pub fn solid(color: Color) -> Self {
        Self::Solid(color)
    }
}
