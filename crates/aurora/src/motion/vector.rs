// Single responsibility: Decomposed motion components interpolated independently.

use crate::foundation::Transform;
use crate::motion::state::MotionState;

/// Wraps a degree delta into the (-180, 180] range.
#[inline]
fn shortest_angle(delta: f32) -> f32 {
    (delta + 180.0).rem_euclid(360.0) - 180.0
}

/// Scalar motion components a controller interpolates independently.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MotionVector {
    /// Horizontal translation in logical pixels.
    pub x: f32,
    /// Vertical translation in logical pixels.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Horizontal scale factor.
    pub scale_x: f32,
    /// Vertical scale factor.
    pub scale_y: f32,
    /// Opacity multiplier.
    pub opacity: f32,
}

impl MotionVector {
    /// Components describing no motion at all.
    pub const NEUTRAL: Self = Self {
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        opacity: 1.0,
    };

    /// Decomposes a motion state into translation, rotation, scale and opacity.
    pub fn from_state(state: &MotionState) -> Self {
        let t = state.transform;
        let scale_x = (t.a * t.a + t.b * t.b).sqrt();
        let scale_y = if scale_x > f32::EPSILON {
            (t.a * t.d - t.b * t.c) / scale_x
        } else {
            1.0
        };
        Self {
            x: t.tx,
            y: t.ty,
            rotation: t.b.atan2(t.a).to_degrees(),
            scale_x,
            scale_y,
            opacity: state.opacity,
        }
    }

    /// Recomposes the components into a motion state.
    pub fn to_state(&self) -> MotionState {
        MotionState {
            transform: Transform::from_translation(self.x, self.y)
                .multiply(&Transform::from_rotation_degrees(self.rotation))
                .multiply(&Transform::from_scale(self.scale_x, self.scale_y)),
            opacity: self.opacity,
        }
    }

    /// Interpolates components, wrapping rotation through its shortest arc.
    pub fn lerp(from: Self, to: Self, t: f32) -> Self {
        Self {
            x: from.x + (to.x - from.x) * t,
            y: from.y + (to.y - from.y) * t,
            rotation: from.rotation + shortest_angle(to.rotation - from.rotation) * t,
            scale_x: from.scale_x + (to.scale_x - from.scale_x) * t,
            scale_y: from.scale_y + (to.scale_y - from.scale_y) * t,
            opacity: from.opacity + (to.opacity - from.opacity) * t,
        }
    }

    /// Components in integration order, shared with the spring track.
    pub(crate) fn as_array(self) -> [f32; 6] {
        [
            self.x,
            self.y,
            self.rotation,
            self.scale_x,
            self.scale_y,
            self.opacity,
        ]
    }

    /// Rebuilds components from integration order.
    pub(crate) fn from_array(values: [f32; 6]) -> Self {
        Self {
            x: values[0],
            y: values[1],
            rotation: values[2],
            scale_x: values[3],
            scale_y: values[4],
            opacity: values[5],
        }
    }
}
