// Single responsibility: Spring tracks settling scalars and rectangles toward a moving target.

use crate::spring::{stable_substeps, Spring};

/// Position error below which a channel counts as at rest.
const POSITION_EPSILON: f32 = 0.005;
/// Velocity below which a channel counts as at rest.
const VELOCITY_EPSILON: f32 = 0.05;

/// Error below which remaining motion is imperceptible and the value is snapped.
/// A soft spring approaches its target asymptotically, so the exact epsilon alone can
/// keep a box animating long after the difference is invisible.
const SNAP_POSITION_EPSILON: f32 = 0.05;
/// Velocity below which remaining motion counts as imperceptible.
const SNAP_VELOCITY_EPSILON: f32 = 0.5;

/// Four-channel vector describing a rectangle's origin and size.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct RectVector {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl RectVector {
    /// A vector with every component at zero.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// Builds a vector from four components.
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Components in integration order.
    fn as_array(self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }

    /// Rebuilds from integration order.
    fn from_array(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }

    /// Component-wise distance used for settle detection.
    fn distance_to(self, other: Self) -> f32 {
        let a = self.as_array();
        let b = other.as_array();
        let mut worst = 0.0f32;
        for i in 0..4 {
            worst = worst.max((a[i] - b[i]).abs());
        }
        worst
    }

    /// Whether every channel has come to rest.
    fn at_rest(&self, target: &Self) -> bool {
        self.distance_to(*target) < POSITION_EPSILON
    }
}

/// Scalar spring track settling one value toward a target while retaining velocity.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScalarTrack {
    /// Current value.
    pub position: f32,
    /// Current value velocity in units per second.
    pub velocity: f32,
    /// Value the track settles toward.
    pub target: f32,
    /// Parameters governing the pull.
    pub spring: Spring,
}

impl ScalarTrack {
    /// Starts a track at `from` heading toward `to`.
    pub fn new(spring: Spring, from: f32, to: f32) -> Self {
        Self {
            position: from,
            velocity: spring.velocity,
            target: to,
            spring,
        }
    }

    /// Retargets in place, preserving position and velocity so interruption never pops.
    pub fn retarget(&mut self, to: f32) {
        self.target = to;
    }

    /// Adopts a launch velocity, for gesture handoff on release.
    ///
    /// A pointer that was moving at 1800px/s when it lifted off should carry that speed into
    /// the spring. Without this the spring would integrate from whatever velocity the
    /// animation happened to have, which for a freshly retargeted track is zero.
    pub fn adopt_velocity(&mut self, velocity: f32) {
        if velocity.is_finite() {
            self.velocity = velocity;
        }
    }

    /// Integrates `dt` seconds with frequency-derived substeps.
    pub fn advance(&mut self, dt: f32) {
        let (steps, step) = stable_substeps(self.spring, dt);
        for _ in 0..steps {
            let mass = self.spring.mass.max(f32::EPSILON);
            let force = self.spring.stiffness * (self.target - self.position)
                - self.spring.damping * self.velocity;
            self.velocity += (force / mass) * step;
            self.position += self.velocity * step;
        }
    }

    /// Whether the track has come to rest within tolerance of its target.
    pub fn is_settled(&self) -> bool {
        (self.position - self.target).abs() < POSITION_EPSILON
            && self.velocity.abs() < VELOCITY_EPSILON
    }
}

/// Four-channel spring track driving a render box toward its flow box.
///
/// Positions are absolute rectangles, so a mid-flight layout change is just a retarget:
/// position and velocity carry over exactly, which is what makes interruption pop-free.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RectTrack {
    /// Current rectangle.
    pub position: RectVector,
    /// Per-channel velocity in units per second.
    pub velocity: RectVector,
    /// Rectangle the track settles toward.
    pub target: RectVector,
    /// Parameters governing the pull.
    pub spring: Spring,
}

impl RectTrack {
    /// Starts a track at `from` heading toward `to`, launching with `velocity`.
    pub fn new(spring: Spring, from: RectVector, to: RectVector, velocity: RectVector) -> Self {
        Self {
            position: from,
            velocity,
            target: to,
            spring,
        }
    }

    /// Retargets in place, preserving position and velocity.
    pub fn retarget(&mut self, to: RectVector) {
        self.target = to;
    }

    /// Adopts a launch velocity, for gesture handoff on release.
    pub fn adopt_velocity(&mut self, velocity: RectVector) {
        self.velocity = velocity;
    }

    /// Integrates `dt` seconds, substepping so stiff springs stay numerically stable.
    pub fn advance(&mut self, dt: f32) {
        let (steps, step) = stable_substeps(self.spring, dt);
        for _ in 0..steps {
            let mut position = self.position.as_array();
            let mut velocity = self.velocity.as_array();
            let target = self.target.as_array();
            let mass = self.spring.mass.max(f32::EPSILON);
            for i in 0..4 {
                let force = self.spring.stiffness * (target[i] - position[i])
                    - self.spring.damping * velocity[i];
                velocity[i] += (force / mass) * step;
                position[i] += velocity[i] * step;
            }
            self.position = RectVector::from_array(position);
            self.velocity = RectVector::from_array(velocity);
        }
    }

    /// Whether every channel has come to rest within tolerance.
    pub fn is_settled(&self) -> bool {
        let velocity = self.velocity.as_array();
        self.position.at_rest(&self.target) && velocity.iter().all(|v| v.abs() < VELOCITY_EPSILON)
    }

    /// Whether remaining motion is small enough to snap away.
    ///
    /// This is the test the render-box pass uses, so a soft spring always terminates
    /// instead of animating toward a target it approaches but never reaches.
    pub fn is_visually_settled(&self) -> bool {
        self.position.distance_to(self.target) < SNAP_POSITION_EPSILON
            && self
                .velocity
                .as_array()
                .iter()
                .all(|v| v.abs() < SNAP_VELOCITY_EPSILON)
    }

    /// Whether the track still differs from its target on any channel.
    pub fn is_moving(&self) -> bool {
        !self.is_visually_settled()
    }

    /// Whether every position and velocity channel is a real number.
    ///
    /// A spring fed a non-finite target never satisfies its settle test, so without this
    /// a single bad value would keep an animation running forever.
    pub fn is_finite(&self) -> bool {
        self.position
            .as_array()
            .iter()
            .chain(self.velocity.as_array().iter())
            .chain(self.target.as_array().iter())
            .all(|v| v.is_finite())
    }

    /// Snaps the track onto `value`, discarding any velocity.
    pub fn reset_to(&mut self, value: RectVector) {
        self.position = value;
        self.velocity = RectVector::ZERO;
        self.target = value;
    }
}
