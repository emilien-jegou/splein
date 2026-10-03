// Single responsibility: Compositor motion overrides composed on top of declarative node style.

use crate::foundation::Transform;

/// Compositor-level overrides applied after declarative layout and style resolve.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MotionState {
    /// Translation, scale and rotation applied on top of the declarative transform.
    pub transform: Transform,
    /// Multiplier applied to the declarative opacity.
    pub opacity: f32,
}

impl MotionState {
    /// Overrides that leave the node visually untouched.
    pub const NEUTRAL: Self = Self {
        transform: Transform::IDENTITY,
        opacity: 1.0,
    };

    /// Whether both overrides are neutral, letting callers skip composition.
    #[inline(always)]
    pub fn is_neutral(&self) -> bool {
        self.transform == Transform::IDENTITY && self.opacity == 1.0
    }

    /// Composes the motion transform onto a declarative transform.
    #[inline(always)]
    pub fn compose_transform(&self, declarative: Transform) -> Transform {
        declarative.multiply(&self.transform)
    }

    /// Composes the motion opacity onto a declarative opacity.
    #[inline(always)]
    pub fn compose_opacity(&self, declarative: f32) -> f32 {
        (declarative * self.opacity).clamp(0.0, 1.0)
    }
}

impl Default for MotionState {
    fn default() -> Self {
        Self::NEUTRAL
    }
}
