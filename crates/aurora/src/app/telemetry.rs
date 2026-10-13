// Single responsibility: Background telemetry worker for asynchronous diagnostics logging.

use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::runtime::FrameDiagnostics;

/// Samples retained for rolling frame statistics (average / p95).
const ROLLING_SAMPLES: usize = 120;

/// Exponential moving average weight for per-frame updates.
const EMA_ALPHA: f64 = 0.1;

/// Asynchronous background worker dispatching frame metrics to Tracy and logging.
pub struct TelemetryWorker {
    sender: SyncSender<FrameDiagnostics>,
    _handle: Option<JoinHandle<()>>,
}

impl TelemetryWorker {
    /// Spawns background telemetry thread with bounded diagnostics channel.
    pub fn spawn() -> Self {
        let (sender, receiver): (SyncSender<FrameDiagnostics>, Receiver<FrameDiagnostics>) =
            sync_channel(32);

        let handle = thread::Builder::new()
            .name("aurora-telemetry".into())
            .spawn(move || Self::worker_loop(receiver))
            .ok();

        Self {
            sender,
            _handle: handle,
        }
    }

    /// Dispatches diagnostics to worker queue without blocking main thread.
    #[inline(always)]
    pub fn send(&self, diag: FrameDiagnostics) {
        match self.sender.try_send(diag) {
            Ok(_) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    fn worker_loop(receiver: Receiver<FrameDiagnostics>) {
        let mut last_activity = Instant::now();
        let mut is_idle = true;
        let mut stats = RollingStats::default();

        loop {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(diag) => {
                    last_activity = Instant::now();
                    is_idle = false;
                    Self::record_diagnostics(&diag, &mut stats);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if last_activity.elapsed() >= Duration::from_millis(200) && !is_idle {
                        Self::record_idle(&mut stats);
                        is_idle = true;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    /// Instantaneous per-phase plots, emitted on the FRAME thread for exact
    /// Tracy timestamps.
    ///
    /// Called from `on_frame` (which runs in the present loop) before the
    /// diagnostics are queued to the worker. The worker keeps only trend plots
    /// (average / p95), where background-thread timestamp skew is harmless.
    /// Without the `tracy` feature this compiles to nothing.
    pub fn plot_frame(diag: &FrameDiagnostics) {
        #[cfg(feature = "tracy")]
        {
            let phases = phase_ms(diag);
            if tracy_client::Client::running().is_some() {
                tracy_client::plot!("Throughput (FPS)", fps_of(phases.total_ms));
                tracy_client::plot!("Frame Total (ms)", phases.total_ms);
                tracy_client::plot!("Layout (ms)", phases.layout_ms);
                tracy_client::plot!("Raster (ms)", phases.raster_ms);
                tracy_client::plot!("Compile (ms)", phases.compile_ms);
                tracy_client::plot!("Present (ms)", phases.present_ms);
                tracy_client::plot!("Reactivity (ms)", phases.reactivity_ms);
            }
        }
        #[cfg(not(feature = "tracy"))]
        {
            let _ = diag;
        }
    }

    fn record_diagnostics(diag: &FrameDiagnostics, stats: &mut RollingStats) {
        let phases = phase_ms(diag);
        stats.push(phases.total_ms, phases.layout_ms, phases.raster_ms);

        #[cfg(feature = "tracy")]
        {
            if tracy_client::Client::running().is_some() {
                tracy_client::plot!("Frame Avg (ms)", stats.avg_total);
                tracy_client::plot!("Frame p95 (ms)", stats.p95_total());
                tracy_client::plot!("Layout Avg (ms)", stats.avg_layout);
                tracy_client::plot!("Raster Avg (ms)", stats.avg_raster);
                tracy_client::plot!("Glyph Cache (MiB)", diag.cache.glyph_bytes as f64 / 1048576.0);
                tracy_client::plot!("Glyph Evictions", diag.cache.glyph_evictions as f64);
            }
        }

        tracing::trace!(
            target: "aurora::telemetry",
            total_ms = phases.total_ms,
            layout_ms = phases.layout_ms,
            raster_ms = phases.raster_ms,
            compile_ms = phases.compile_ms,
            present_ms = phases.present_ms,
            reactivity_ms = phases.reactivity_ms,
            throughput_fps = fps_of(phases.total_ms),
            frame_avg_ms = stats.avg_total,
            frame_p95_ms = stats.p95_total(),
            glyph_cache_mib = diag.cache.glyph_bytes as f64 / 1048576.0,
            glyph_cache_cap_mib = diag.cache.glyph_cap_bytes as f64 / 1048576.0,
            glyph_entries = diag.cache.glyph_entries,
            glyph_evictions = diag.cache.glyph_evictions,
            "worker_frame"
        );
    }

    fn record_idle(stats: &mut RollingStats) {
        // An idle gap ends the rolling window: stale samples must not pollute
        // the average when frames resume.
        *stats = RollingStats::default();
        #[cfg(feature = "tracy")]
        {
            if tracy_client::Client::running().is_some() {
                tracy_client::plot!("Throughput (FPS)", 0.0);
                tracy_client::plot!("Frame Total (ms)", 0.0);
                tracy_client::plot!("Layout (ms)", 0.0);
                tracy_client::plot!("Raster (ms)", 0.0);
                tracy_client::plot!("Compile (ms)", 0.0);
                tracy_client::plot!("Present (ms)", 0.0);
                tracy_client::plot!("Reactivity (ms)", 0.0);
                tracy_client::plot!("Frame Avg (ms)", 0.0);
                tracy_client::plot!("Frame p95 (ms)", 0.0);
                tracy_client::plot!("Layout Avg (ms)", 0.0);
                tracy_client::plot!("Raster Avg (ms)", 0.0);
            }
        }
    }
}

/// Per-phase frame timings in milliseconds, on the calling thread.
struct PhaseMs {
    total_ms: f64,
    layout_ms: f64,
    raster_ms: f64,
    compile_ms: f64,
    present_ms: f64,
    reactivity_ms: f64,
}

fn phase_ms(diag: &FrameDiagnostics) -> PhaseMs {
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    PhaseMs {
        total_ms: ms(diag.timings.total),
        layout_ms: ms(diag.timings.layout),
        raster_ms: ms(diag.timings.raster),
        compile_ms: ms(diag.timings.compile),
        present_ms: ms(diag.timings.present),
        reactivity_ms: ms(diag.timings.reactivity),
    }
}

fn fps_of(total_ms: f64) -> f64 {
    if total_ms > 0.0 {
        (1000.0 / total_ms).min(10000.0)
    } else {
        0.0
    }
}

/// Rolling frame statistics: exponential moving averages plus a bounded
/// window of totals for p95. Trend data only — never exact per-frame truth.
#[derive(Default)]
struct RollingStats {
    totals: VecDeque<f64>,
    avg_total: f64,
    avg_layout: f64,
    avg_raster: f64,
}

impl RollingStats {
    fn push(&mut self, total_ms: f64, layout_ms: f64, raster_ms: f64) {
        let ema = |avg: f64, sample: f64| {
            if avg == 0.0 {
                sample
            } else {
                avg * (1.0 - EMA_ALPHA) + sample * EMA_ALPHA
            }
        };
        self.avg_total = ema(self.avg_total, total_ms);
        self.avg_layout = ema(self.avg_layout, layout_ms);
        self.avg_raster = ema(self.avg_raster, raster_ms);
        self.totals.push_back(total_ms);
        while self.totals.len() > ROLLING_SAMPLES {
            self.totals.pop_front();
        }
    }

    fn p95_total(&self) -> f64 {
        if self.totals.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = self.totals.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((sorted.len() as f64 * 0.95).ceil() as usize)
            .clamp(1, sorted.len())
            - 1;
        sorted[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ema_converges_toward_samples() {
        let mut stats = RollingStats::default();
        assert_eq!(stats.p95_total(), 0.0);
        for _ in 0..200 {
            stats.push(10.0, 4.0, 5.0);
        }
        assert!((stats.avg_total - 10.0).abs() < 0.01);
        assert!((stats.avg_layout - 4.0).abs() < 0.01);
        assert!((stats.avg_raster - 5.0).abs() < 0.01);
        assert!((stats.p95_total() - 10.0).abs() < 0.01);
    }

    #[test]
    fn p95_ignores_fast_majority() {
        let mut stats = RollingStats::default();
        for _ in 0..100 {
            stats.push(10.0, 1.0, 1.0);
        }
        for _ in 0..10 {
            stats.push(100.0, 1.0, 1.0);
        }
        // 95th percentile of 110 samples lands on a slow sample...
        assert!((stats.p95_total() - 100.0).abs() < 0.01);
        // ...while the average stays well below the tail.
        assert!(stats.avg_total < stats.p95_total());
    }

    #[test]
    fn phase_ms_and_fps_math() {
        let diag = FrameDiagnostics {
            timings: crate::runtime::FrameTimings {
                total: Duration::from_millis(20),
                layout: Duration::from_millis(5),
                ..Default::default()
            },
            ..Default::default()
        };
        let phases = phase_ms(&diag);
        assert!((phases.total_ms - 20.0).abs() < 0.01);
        assert!((phases.layout_ms - 5.0).abs() < 0.01);
        assert!((fps_of(20.0) - 50.0).abs() < 0.01);
        assert_eq!(fps_of(0.0), 0.0);
    }
}
