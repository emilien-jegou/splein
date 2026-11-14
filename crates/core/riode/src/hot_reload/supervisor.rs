// Responsibility: Pure event-driven state machine coordinating file watching, freshness checks, and compilation.

use super::builder::{read_tail, BuildProcess, BuildRunner, BuildStatus};
use super::depinfo::parse_depinfo;
use super::dylib::DylibReloader;
use super::events::{SupervisorEvent, SupervisorState};
use super::freshness::{decide_freshness, max_mtime_under, FreshDecision};
use super::noise::is_temp_noise;
use super::watcher::FileWatcher;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

const FRESHNESS_INTERVAL: Duration = Duration::from_secs(1);
const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(150);

pub struct DevSupervisor {
    runner: Box<dyn BuildRunner>,
    loader: DylibReloader,
    watcher: FileWatcher,
    active_build: Option<Box<dyn BuildProcess>>,
    build_started: Option<Instant>,
    build_log: PathBuf,
    source_files: Vec<PathBuf>,
    last_trigger_sources: Option<SystemTime>,
    last_freshness_check: Instant,
    last_change_time: Option<Instant>,
    dirty_during_build: bool,
    has_failure: bool,
}

impl DevSupervisor {
    pub fn new(runner: Box<dyn BuildRunner>) -> Self {
        let watch_dir = runner.watch_dir();
        let manifest_path = runner.manifest_path();
        let lib_path = runner.lib_path();

        let mut watcher = FileWatcher::new();
        watcher.watch(&watch_dir);
        watcher.watch(&manifest_path);

        tracing::info!(
            "riode: hot-reload armed for `{}` | watching: {} | target: {}",
            runner.target_id(),
            watch_dir.display(),
            lib_path.display()
        );

        let build_log = std::env::temp_dir().join(format!("riode-build-{}.log", std::process::id()));

        Self {
            runner,
            loader: DylibReloader::new(lib_path),
            watcher,
            active_build: None,
            build_started: None,
            build_log,
            source_files: Vec::new(),
            last_trigger_sources: None,
            last_freshness_check: Instant::now() - FRESHNESS_INTERVAL,
            last_change_time: None,
            dirty_during_build: false,
            has_failure: false,
        }
    }

    pub fn loader(&self) -> &DylibReloader {
        &self.loader
    }

    pub fn poll(&mut self) -> SupervisorEvent {
        let changed: Vec<_> = self
            .watcher
            .poll_changed()
            .into_iter()
            .filter(|p| !is_temp_noise(p))
            .collect();
        let watcher_hit = !changed.is_empty();

        if watcher_hit {
            tracing::debug!("riode: hot-reload detected changes: {changed:?}");
            self.last_change_time = Some(Instant::now());
        }

        if self.active_build.is_some() {
            if watcher_hit {
                self.dirty_during_build = true;
            }
            return self.poll_active_build();
        }

        if let Some(change_time) = self.last_change_time {
            if change_time.elapsed() < DEBOUNCE_INTERVAL {
                return SupervisorEvent::Building;
            }
            self.last_change_time = None;
            self.spawn_build("sources changed");
            if self.active_build.is_some() {
                return SupervisorEvent::Building;
            }
        }

        if self.last_freshness_check.elapsed() >= FRESHNESS_INTERVAL {
            if let Some(max_src) = self.max_source_mtime() {
                let (decision, baseline) = decide_freshness(
                    max_src,
                    self.last_trigger_sources,
                    self.loader.loaded_mtime(),
                );
                self.last_trigger_sources = baseline;
                if decision == FreshDecision::Build {
                    self.spawn_build("sources newer than image");
                    if self.active_build.is_some() {
                        return SupervisorEvent::Building;
                    }
                }
            }
            self.last_freshness_check = Instant::now();
        }

        if self.loader.refresh() {
            self.refresh_source_set();
            self.has_failure = false;
            return SupervisorEvent::LibraryReady;
        }

        SupervisorEvent::Idle
    }

    fn poll_active_build(&mut self) -> SupervisorEvent {
        let Some(build) = self.active_build.as_mut() else {
            return SupervisorEvent::Idle;
        };
        match build.poll_status() {
            Ok(BuildStatus::Running) => SupervisorEvent::Building,
            Ok(BuildStatus::Success) => {
                self.active_build = None;
                let elapsed = self.build_started.take().map(|t| t.elapsed()).unwrap_or_default();
                tracing::info!("riode: hot-reload build finished in {:.2}s", elapsed.as_secs_f32());

                if self.dirty_during_build {
                    self.dirty_during_build = false;
                    self.spawn_build("sources changed during build");
                    return SupervisorEvent::Building;
                }
                if self.loader.refresh() {
                    self.has_failure = false;
                    SupervisorEvent::LibraryReady
                } else {
                    tracing::info!("riode: build succeeded but output library unchanged");
                    SupervisorEvent::BuildSettled
                }
            }
            Ok(BuildStatus::Failed { code }) => {
                self.active_build = None;
                self.build_started = None;
                self.has_failure = true;
                let log = read_tail(&self.build_log, 20);
                tracing::warn!("riode: hot-reload build failed (exit {code:?})");
                for line in &log {
                    tracing::warn!("riode: build | {line}");
                }
                SupervisorEvent::BuildFailed {
                    cause: format!("build failed for {}", self.runner.target_id()),
                    log,
                }
            }
            Err(e) => {
                self.active_build = None;
                self.build_started = None;
                tracing::warn!("riode: build poll error: {e}");
                SupervisorEvent::BuildSettled
            }
        }
    }

    fn spawn_build(&mut self, reason: &str) {
        if self.active_build.is_none() {
            match self.runner.spawn_build(&self.build_log, reason) {
                Ok(process) => {
                    tracing::info!(
                        "riode: hot-reload building `{}` ({reason}); log: {}",
                        self.runner.target_id(),
                        self.build_log.display()
                    );
                    self.build_started = Some(Instant::now());
                    self.active_build = Some(process);
                }
                Err(e) => {
                    tracing::error!(
                        "riode: failed to spawn build for `{}`: {e}",
                        self.runner.target_id()
                    );
                }
            }
        }
    }

    fn max_source_mtime(&self) -> Option<SystemTime> {
        if self.source_files.is_empty() {
            max_mtime_under(&self.runner.watch_dir())
        } else {
            self.source_files
                .iter()
                .filter_map(|f| std::fs::metadata(f).ok()?.modified().ok())
                .max()
        }
    }

    fn refresh_source_set(&mut self) {
        let depinfo = self.runner.lib_path().with_extension("d");
        if let Ok(text) = std::fs::read_to_string(&depinfo) {
            let files = parse_depinfo(&text);
            for f in &files {
                self.watcher.watch(f);
            }
            self.source_files = files;
        }
    }

    pub fn state(&self) -> SupervisorState {
        if self.has_failure {
            SupervisorState::Failed
        } else if self.active_build.is_some() {
            SupervisorState::Building
        } else {
            SupervisorState::Watching
        }
    }
}
