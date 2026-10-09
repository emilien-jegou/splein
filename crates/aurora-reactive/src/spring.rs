// Single responsibility: Spring parameters and the stateful simulation that settles them.

/// Longest frame delta simulated, so a stalled frame cannot teleport a spring.
const MAX_DELTA_SECS: f32 = 0.1;
/// Radians of oscillation per substep, well inside the integrator's stability bound of 2.
const RADIAN_BUDGET: f32 = 0.25;
/// Upper bound on substeps, which only engages for pathologically stiff springs.
const MAX_STEPS: u32 = 64;

/// Splits `dt` into numerically stable substeps for an explicit integration of `spring`.
pub(crate) fn stable_substeps(spring: Spring, dt: f32) -> (u32, f32) {
    let dt = dt.min(MAX_DELTA_SECS);
    if dt <= 0.0 {
        return (0, 0.0);
    }
    let frequency = (spring.stiffness / spring.mass.max(f32::EPSILON)).sqrt();
    let budgeted = if frequency > 0.0 {
        RADIAN_BUDGET / frequency
    } else {
        dt
    };
    let steps = (dt / budgeted).ceil().clamp(1.0, MAX_STEPS as f32) as u32;
    (steps, dt / steps as f32)
}

/// Tunable spring governing physics-based settling.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Spring {
    /// Restoring force in units per second squared per unit displacement.
    pub stiffness: f32,
    /// Velocity damping coefficient resisting motion.
    pub damping: f32,
    /// Inertial mass resisting acceleration.
    pub mass: f32,
    /// Velocity imparted the moment the spring starts moving.
    pub velocity: f32,
}

impl Spring {
    /// Fast settle carrying a slight overshoot.
    pub const fn snappy() -> Self {
        Self::new(400.0, 34.0, 1.0)
    }

    /// Playful settle with pronounced overshoot.
    pub const fn bouncy() -> Self {
        Self::new(250.0, 14.0, 1.0)
    }

    /// Soft settle that never crosses its target.
    pub const fn smooth() -> Self {
        Self::new(180.0, 30.0, 1.0)
    }

    /// Creates a spring from explicit physics parameters.
    pub const fn new(stiffness: f32, damping: f32, mass: f32) -> Self {
        Self {
            stiffness,
            damping,
            mass,
            velocity: 0.0,
        }
    }

    /// Returns a copy launching with the given velocity, for gesture handoff.
    pub const fn with_velocity(self, velocity: f32) -> Self {
        Self {
            stiffness: self.stiffness,
            damping: self.damping,
            mass: self.mass,
            velocity,
        }
    }

    /// Damping ratio: below one overshoots, one is critical, above one is overdamped.
    pub fn damping_ratio(&self) -> f32 {
        self.damping / (2.0 * (self.stiffness * self.mass).sqrt())
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::snappy()
    }
}

/// Stateful spring simulation integrating a position toward a target.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SpringState {
    /// Current simulated position.
    pub position: f32,
    /// Current simulated velocity in units per second.
    pub velocity: f32,
    /// Rest position the spring is pulled toward.
    pub target: f32,
    /// Parameters governing the pull.
    pub spring: Spring,
}

impl SpringState {
    /// Position error below which the spring counts as at rest.
    pub const POSITION_EPSILON: f32 = 0.005;
    /// Velocity below which the spring counts as at rest.
    pub const VELOCITY_EPSILON: f32 = 0.05;

    /// Starts a spring at `from` heading toward `to` with the spring's launch velocity.
    pub fn new(spring: Spring, from: f32, to: f32) -> Self {
        Self {
            position: from,
            velocity: spring.velocity,
            target: to,
            spring,
        }
    }

    /// Retargets in place, preserving instantaneous position and velocity on interruption.
    pub fn retarget(&mut self, to: f32) {
        self.target = to;
    }

    /// Integrates `dt` seconds, substepping so stiff springs stay numerically stable.
    pub fn advance(&mut self, dt: f32) {
        let (steps, step) = stable_substeps(self.spring, dt);
        for _ in 0..steps {
            self.integrate(step);
        }
    }

    /// Whether the spring has come to rest within tolerance of its target.
    pub fn is_settled(&self) -> bool {
        (self.position - self.target).abs() < Self::POSITION_EPSILON
            && self.velocity.abs() < Self::VELOCITY_EPSILON
    }

    /// Applies one explicit Euler step of `step` seconds.
    fn integrate(&mut self, step: f32) {
        let mass = self.spring.mass.max(f32::EPSILON);
        let force = self.spring.stiffness * (self.target - self.position)
            - self.spring.damping * self.velocity;
        self.velocity += (force / mass) * step;
        self.position += self.velocity * step;
    }
}
