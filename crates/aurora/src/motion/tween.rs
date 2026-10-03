// Single responsibility: Duration-bounded interpolation between two values under an easing curve.

use crate::motion::ease::Ease;

/// Duration-bounded interpolation between two values under an easing curve.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Tween {
    /// Total duration in seconds.
    pub duration: f32,
    /// Easing curve applied to normalized elapsed time.
    pub ease: Ease,
}

impl Tween {
    /// Creates a tween lasting `duration` seconds under the given curve.
    pub const fn new(duration: f32, ease: Ease) -> Self {
        Self { duration, ease }
    }

    /// Linear fraction of the tween elapsed, clamped to 0..=1.
    pub fn fraction(&self, elapsed: f32) -> f32 {
        if self.duration > 0.0 {
            (elapsed / self.duration).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// Eased progress for `elapsed` seconds.
    pub fn progress(&self, elapsed: f32) -> f32 {
        self.ease.sample(self.fraction(elapsed))
    }

    /// Whether `elapsed` seconds has reached the full duration.
    pub fn is_complete(&self, elapsed: f32) -> bool {
        elapsed >= self.duration
    }

    /// Interpolates between `from` and `to` at `elapsed` seconds.
    pub fn lerp(&self, elapsed: f32, from: f32, to: f32) -> f32 {
        from + (to - from) * self.progress(elapsed)
    }
}

/// Duration sugar so animation call sites can read like `300.ms()`.
pub trait Millis {
    /// Converts a millisecond count into seconds.
    fn ms(self) -> f32;
}

impl Millis for f32 {
    /// Converts a fractional millisecond count into seconds.
    fn ms(self) -> f32 {
        self / 1000.0
    }
}

impl Millis for u32 {
    /// Converts a whole millisecond count into seconds.
    fn ms(self) -> f32 {
        self as f32 / 1000.0
    }
}
