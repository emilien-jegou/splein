// Single responsibility: Frame delta source with a deterministic override for headless stepping.

use std::time::Instant;

/// Longest measured delta accepted, so a stall cannot teleport an animation.
const MAX_DELTA_SECS: f32 = 0.1;

/// Frame delta source yielding either a forced or a measured delta in seconds.
pub struct Clock {
    previous: Instant,
    forced: Option<f32>,
}

impl Clock {
    /// Creates a clock measuring its first delta from now.
    pub fn new() -> Self {
        Self {
            previous: Instant::now(),
            forced: None,
        }
    }

    /// Forces the next delta, so headless tests can step time deterministically.
    pub fn force(&mut self, dt: f32) {
        self.forced = Some(dt);
    }

    /// Consumes the next delta in seconds.
    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let measured = (now - self.previous).as_secs_f32();
        self.previous = now;
        match self.forced.take() {
            Some(dt) => dt.max(0.0),
            None => measured.min(MAX_DELTA_SECS),
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}
