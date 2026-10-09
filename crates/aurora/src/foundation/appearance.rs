// Aggregate visual surface appearance specification.

use crate::foundation::fill::Fill;
use crate::foundation::light::LightEnvironment;
use crate::foundation::radius::Radius;
use crate::foundation::shadow::Shadow;
use crate::foundation::stroke::Stroke;

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub radius: Radius,
    /// One shadow per node, internally layered.
    pub shadow: Option<Shadow>,
    /// Per-node light override; defaults to [`LightEnvironment::DEFAULT`].
    pub light: LightEnvironment,
    pub opacity: f32,
}

impl Appearance {
    pub const EMPTY: Self = Self {
        fill: None,
        stroke: None,
        radius: Radius::ZERO,
        shadow: None,
        light: LightEnvironment::DEFAULT,
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
        self.shadow = Some(shadow);
        self
    }

    pub fn with_light(mut self, light: LightEnvironment) -> Self {
        self.light = light;
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
