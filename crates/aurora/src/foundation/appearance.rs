// Aggregate visual surface appearance specification.

use crate::foundation::fill::Fill;
use crate::foundation::radius::Radius;
use crate::foundation::shadow::Shadow;
use crate::foundation::stroke::Stroke;

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub radius: Radius,
    pub shadows: Vec<Shadow>,
    pub opacity: f32,
}

impl Appearance {
    pub const EMPTY: Self = Self {
        fill: None,
        stroke: None,
        radius: Radius::ZERO,
        shadows: Vec::new(),
        opacity: 1.0,
    };

    pub fn with_fill(mut self, fill: Fill) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn with_radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self::EMPTY
    }
}
