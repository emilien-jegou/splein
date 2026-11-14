// Responsibility: Stores telemetry, performance metrics, and build failure diagnostics for hot reload.

use super::events::SupervisorState;
use std::time::Duration;

/// Outcome report of one completed hot-reload pass.
#[derive(Clone, Debug)]
pub struct ReloadReport {
    pub reloaded: Vec<String>,
    pub duration: Duration,
    pub error: Option<String>,
    pub laid_out: bool,
    pub nodes_dirtied: usize,
    pub damage_rects: usize,
}

/// Cumulative hot-reload performance metrics and operational diagnostics.
#[derive(Clone, Debug, Default)]
pub struct HotReloadStats {
    pub reloads_total: u64,
    pub reloads_failed: u64,
    pub reloaded_log: Vec<String>,
    pub last_duration_ms: f64,
    pub last_reloaded: Vec<String>,
    pub last_error: Option<String>,
    pub last_build_error: Option<String>,
    pub last_build_log: Vec<String>,
    pub components_tracked: usize,
    pub supervisor_state: SupervisorState,
}

impl HotReloadStats {
    pub fn record_report(&mut self, report: &ReloadReport) {
        if report.error.is_some() {
            self.reloads_failed += 1;
        } else {
            self.reloads_total += 1;
        }
        self.last_duration_ms = report.duration.as_secs_f64() * 1000.0;
        self.last_reloaded = report.reloaded.clone();
        self.last_error = report.error.clone();
        self.reloaded_log.extend(report.reloaded.iter().cloned());
    }

    pub fn record_build_failure(&mut self, cause: String, log: Vec<String>) {
        self.last_build_error = Some(cause);
        self.last_build_log = log;
        self.supervisor_state = SupervisorState::Failed;
    }
}
