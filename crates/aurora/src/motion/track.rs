// Single responsibility: Vector-valued spring track settling motion components toward a target.

use crate::motion::spring::{stable_substeps, Spring, SpringState};
use crate::motion::vector::MotionVector;

/// Vector-valued spring track settling every motion component toward one target.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SpringTrack {
    /// Current component values.
    pub position: MotionVector,
    /// Current component velocities in units per second.
    pub velocity: MotionVector,
    /// Component values the track settles toward.
    pub target: MotionVector,
    /// Parameters governing the pull.
    pub spring: Spring,
}

impl SpringTrack {
    /// Starts a track at `from` heading toward `to`, launching on the horizontal channel.
    pub fn new(spring: Spring, from: MotionVector, to: MotionVector) -> Self {
        Self {
            position: from,
            velocity: MotionVector {
                x: spring.velocity,
                ..MotionVector::NEUTRAL
            },
            target: to,
            spring,
        }
    }

    /// Retargets in place, preserving instantaneous position and velocity.
    pub fn retarget(&mut self, to: MotionVector) {
        self.target = to;
    }

    /// Integrates `dt` seconds with frequency-derived substeps.
    pub fn advance(&mut self, dt: f32) {
        let (steps, step) = stable_substeps(self.spring, dt);
        for _ in 0..steps {
            self.integrate(step);
        }
    }

    /// Whether every component has come to rest within tolerance.
    pub fn is_settled(&self) -> bool {
        let position = self.position.as_array();
        let target = self.target.as_array();
        let velocity = self.velocity.as_array();
        position
            .iter()
            .zip(target.iter())
            .all(|(p, t)| (p - t).abs() < SpringState::POSITION_EPSILON)
            && velocity.iter().all(|v| v.abs() < SpringState::VELOCITY_EPSILON)
    }

    /// Applies one explicit Euler step toward the target.
    fn integrate(&mut self, step: f32) {
        let mass = self.spring.mass.max(f32::EPSILON);
        let mut position = self.position.as_array();
        let mut velocity = self.velocity.as_array();
        let target = self.target.as_array();
        for (index, component) in target.iter().enumerate() {
            let force = self.spring.stiffness * (component - position[index])
                - self.spring.damping * velocity[index];
            velocity[index] += (force / mass) * step;
            position[index] += velocity[index] * step;
        }
        self.position = MotionVector::from_array(position);
        self.velocity = MotionVector::from_array(velocity);
    }
}
