// Single responsibility: Frame rate pacing policy and JIT late-latch frame deadline calculation.

use std::time::{Duration, Instant};
use winit::window::Window;

/// Frame rate pacing limit policy.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum FpsLimit {
    /// Locks presentation to display refresh rate.
    #[default]
    Auto,
    /// Caps presentation to an explicit target FPS.
    Custom(u32),
    /// Uncapped continuous presentation without frame sleeps.
    Uncapped,
}

impl From<u32> for FpsLimit {
    fn from(fps: u32) -> Self {
        Self::Custom(fps)
    }
}

impl From<Option<u32>> for FpsLimit {
    fn from(opt: Option<u32>) -> Self {
        match opt {
            Some(fps) => Self::Custom(fps),
            None => Self::Uncapped,
        }
    }
}

/// Computes the target frame interval duration from an FPS policy and window monitor.
pub fn compute_target_interval(limit: FpsLimit, window: &Window) -> Option<Duration> {
    let target_fps = match limit {
        FpsLimit::Auto => window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| (mhz as f64 / 1000.0).round() as u32)
            .or(Some(240)),
        FpsLimit::Custom(fps) => Some(fps),
        FpsLimit::Uncapped => None,
    };
    target_fps.map(|fps| Duration::from_secs_f64(1.0 / fps.max(1) as f64))
}

/// Computes the JIT wake deadline accounting for moving average compute duration.
pub fn compute_jit_wake(
    last_present: Instant,
    interval: Duration,
    avg_compute: Duration,
) -> Instant {
    let lead_time = (avg_compute + Duration::from_millis(1)).min(interval.mul_f32(0.85));
    let deadline = last_present + interval;
    deadline.checked_sub(lead_time).unwrap_or(last_present)
}
