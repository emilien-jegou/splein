// Single responsibility: Named easing curves mapping linear progress onto eased progress.

/// Cubic ease-in curve.
#[inline]
fn cubic_in(t: f32) -> f32 {
    t * t * t
}

/// Quintic ease-in curve.
#[inline]
fn quintic_in(t: f32) -> f32 {
    t * t * t * t * t
}

/// Folds a one-directional curve into a symmetric accelerate-then-decelerate curve.
#[inline]
fn in_out(t: f32, curve: impl Fn(f32) -> f32) -> f32 {
    if t < 0.5 {
        curve(t * 2.0) * 0.5
    } else {
        1.0 - curve((1.0 - t) * 2.0) * 0.5
    }
}

/// Named easing curves interpolating normalized progress.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Ease {
    /// Unaccelerated straight-line interpolation.
    Linear,
    /// Accelerates slowly into the target.
    InCubic,
    /// Arrives fast, then decelerates into the target.
    #[default]
    OutCubic,
    /// Accelerates through the first half and decelerates through the second.
    InOutCubic,
    /// Strong deceleration for snappy arrivals.
    OutQuint,
    /// Symmetric quintic acceleration and deceleration.
    InOutQuint,
    /// Covers most of the distance almost immediately, then glides to rest.
    OutExpo,
}

impl Ease {
    /// Maps linear progress in 0..=1 onto its eased value, clamping out-of-range input.
    #[inline]
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::InCubic => cubic_in(t),
            Ease::OutCubic => 1.0 - cubic_in(1.0 - t),
            Ease::InOutCubic => in_out(t, cubic_in),
            Ease::OutQuint => 1.0 - quintic_in(1.0 - t),
            Ease::InOutQuint => in_out(t, quintic_in),
            Ease::OutExpo => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - (-10.0 * t).exp2()
                }
            }
        }
    }
}
