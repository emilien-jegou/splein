// Single responsibility: Interpolation modes producing a value and a completion flag.

use crate::motion::spring::Spring;
use crate::motion::track::SpringTrack;
use crate::motion::tween::Tween;
use crate::motion::vector::MotionVector;

/// Interpolation mode driving a controller toward its target.
pub(crate) enum Mode {
    /// Fixed-duration eased interpolation with elapsed seconds.
    Tween { tween: Tween, elapsed: f32 },
    /// Physics-based settling.
    Spring(SpringTrack),
}

impl Mode {
    /// Creates a timed tween mode.
    pub(crate) fn tween(tween: Tween) -> Self {
        Self::Tween {
            tween,
            elapsed: 0.0,
        }
    }

    /// Creates a physics-based spring mode.
    pub(crate) fn spring(spring: Spring, from: MotionVector, to: MotionVector) -> Self {
        Self::Spring(SpringTrack::new(spring, from, to))
    }

    /// Advances by `dt` seconds, returning the value and whether it settled.
    pub(crate) fn step(
        &mut self,
        from: MotionVector,
        to: MotionVector,
        dt: f32,
    ) -> (MotionVector, bool) {
        match self {
            Self::Tween { tween, elapsed } => {
                *elapsed += dt;
                let settled = tween.is_complete(*elapsed);
                let value = MotionVector::lerp(from, to, tween.progress(*elapsed));
                (value, settled)
            }
            Self::Spring(track) => {
                track.advance(dt);
                let settled = track.is_settled();
                let value = if settled { track.target } else { track.position };
                (value, settled)
            }
        }
    }

    /// Value presented while the mode is not advancing.
    pub(crate) fn value(&self, from: MotionVector, to: MotionVector) -> MotionVector {
        match self {
            Self::Tween { tween, elapsed } => {
                MotionVector::lerp(from, to, tween.progress(*elapsed))
            }
            Self::Spring(track) => track.position,
        }
    }

    /// Component velocities, which a retarget carries over.
    pub(crate) fn velocity(&self) -> MotionVector {
        match self {
            Self::Spring(track) => track.velocity,
            Self::Tween { .. } => MotionVector::NEUTRAL,
        }
    }

    /// Adopts a launch velocity before the first step of a retargeted spring.
    pub(crate) fn adopt_velocity(&mut self, velocity: MotionVector) {
        if let Self::Spring(track) = self {
            track.velocity = velocity;
        }
    }
}
