// Single responsibility: Background telemetry worker for asynchronous diagnostics logging.

use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::runtime::FrameDiagnostics;

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

        loop {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(diag) => {
                    last_activity = Instant::now();
                    is_idle = false;
                    Self::record_diagnostics(&diag);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if last_activity.elapsed() >= Duration::from_millis(200) && !is_idle {
                        Self::record_idle();
                        is_idle = true;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn record_diagnostics(diag: &FrameDiagnostics) {
        let total_ms = diag.timings.total.as_secs_f64() * 1000.0;
        let layout_ms = diag.timings.layout.as_secs_f64() * 1000.0;
        let raster_ms = diag.timings.raster.as_secs_f64() * 1000.0;
        let throughput_fps = if total_ms > 0.0 {
            (1000.0 / total_ms).min(10000.0)
        } else {
            0.0
        };

        #[cfg(feature = "tracy")]
        {
            tracy_client::plot!("Throughput (FPS)", throughput_fps);
            tracy_client::plot!("Frame Total (ms)", total_ms);
            tracy_client::plot!("Layout (ms)", layout_ms);
            tracy_client::plot!("Raster (ms)", raster_ms);
        }

        tracing::trace!(
            target: "aurora::telemetry",
            total_ms = total_ms,
            layout_ms = layout_ms,
            raster_ms = raster_ms,
            throughput_fps = throughput_fps,
            "worker_frame"
        );
    }

    fn record_idle() {
        #[cfg(feature = "tracy")]
        {
            tracy_client::plot!("Throughput (FPS)", 0.0);
            tracy_client::plot!("Frame Total (ms)", 0.0);
            tracy_client::plot!("Layout (ms)", 0.0);
            tracy_client::plot!("Raster (ms)", 0.0);
        }
    }
}
