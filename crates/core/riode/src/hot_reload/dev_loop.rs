// Responsibility: Coordinates file change detection, build process polling, and freshness reconciliation.

use super::builder::{read_tail, BuildRunner};
use super::depinfo::parse_depinfo;
use super::dylib::DylibReloader;
use super::freshness::{decide_freshness, max_mtime_under, FreshDecision};
use super::noise::is_temp_noise;
use super::watcher::FileWatcher;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant, SystemTime};

const FRESHNESS_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    LibraryReady,
    Settled,
    Building,
    Idle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevLoopState {
    #[default]
    Watching,
    Stale,
    Building,
    Failed,
}

pub struct DevLoop {
    runner: Box<dyn BuildRunner>,
    loader: DylibReloader,
    watcher: FileWatcher,
    build_child: Option<Child>,
    build_log: PathBuf,
    build_error: Option<(String, Vec<String>)>,
    source_files: Vec<PathBuf>,
    last_trigger_sources: Option<SystemTime>,
    last_max_sources: Option<SystemTime>,
    last_freshness_check: Instant,
    dirty_during_build: bool,
}

impl DevLoop {
    pub fn new(runner: Box<dyn BuildRunner>) -> Self {
        let mut watcher = FileWatcher::new();
        watcher.watch(runner.watch_dir());
        watcher.watch(runner.manifest_path());
        let build_log = std::env::temp_dir().join(format!("riode-build-{}.log", std::process::id()));
        let lib_path = runner.lib_path();

        Self {
            runner,
            loader: DylibReloader::new(lib_path),
            watcher,
            build_child: None,
            build_log,
            build_error: None,
            source_files: Vec::new(),
            last_trigger_sources: None,
            last_max_sources: None,
            last_freshness_check: Instant::now() - FRESHNESS_INTERVAL,
            dirty_during_build: false,
        }
    }

    pub fn loader(&self) -> &DylibReloader { &self.loader }
    pub fn take_build_error(&mut self) -> Option<(String, Vec<String>)> { self.build_error.take() }

    pub fn poll(&mut self) -> PollOutcome {
        let changed: Vec<_> = self.watcher.poll_changed().into_iter().filter(|p| !is_temp_noise(p)).collect();
        let watcher_hit = !changed.is_empty();

        if self.build_child.is_some() {
            if watcher_hit { self.dirty_during_build = true; }
            return self.poll_build();
        }

        let mut trigger = watcher_hit;
        let mut reason = "sources changed";
        if !trigger && self.last_freshness_check.elapsed() >= FRESHNESS_INTERVAL {
            if let Some(max_src) = self.max_source_mtime() {
                let (decision, baseline) = decide_freshness(max_src, self.last_trigger_sources, self.loader.loaded_mtime());
                self.last_trigger_sources = baseline;
                if decision == FreshDecision::Build {
                    trigger = true;
                    reason = "sources newer than image";
                }
            }
            self.last_freshness_check = Instant::now();
        }

        if trigger {
            self.spawn_build(reason);
            if self.build_child.is_some() { return PollOutcome::Building; }
        }

        if self.loader.refresh() {
            self.refresh_source_set();
            return PollOutcome::LibraryReady;
        }

        if !self.loader.is_loaded() {
            self.spawn_build("initial");
            if self.build_child.is_some() { return PollOutcome::Building; }
        }

        PollOutcome::Idle
    }

    fn poll_build(&mut self) -> PollOutcome {
        let Some(child) = self.build_child.as_mut() else { return PollOutcome::Idle };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.build_child = None;
                if !status.success() {
                    let log = read_tail(&self.build_log, 20);
                    self.build_error = Some((format!("build failed for {}", self.runner.target_id()), log));
                    return PollOutcome::Settled;
                }
                if self.dirty_during_build {
                    self.dirty_during_build = false;
                    self.spawn_build("sources changed during build");
                    return PollOutcome::Building;
                }
                if self.loader.refresh() { PollOutcome::LibraryReady } else { PollOutcome::Settled }
            }
            Ok(None) => PollOutcome::Building,
            Err(_) => {
                self.build_child = None;
                PollOutcome::Settled
            }
        }
    }

    fn spawn_build(&mut self, reason: &str) {
        if self.build_child.is_none() {
            if let Ok(child) = self.runner.spawn_build(&self.build_log, reason) {
                self.build_child = Some(child);
            }
        }
    }

    fn max_source_mtime(&mut self) -> Option<SystemTime> {
        let max = if self.source_files.is_empty() {
            max_mtime_under(&self.runner.watch_dir())
        } else {
            self.source_files.iter().filter_map(|f| std::fs::metadata(f).ok()?.modified().ok()).max()
        };
        self.last_max_sources = max;
        max
    }

    fn refresh_source_set(&mut self) {
        let depinfo = self.runner.lib_path().with_extension("d");
        if let Ok(text) = std::fs::read_to_string(&depinfo) {
            let files = parse_depinfo(&text);
            for f in &files { self.watcher.watch(f); }
            self.source_files = files;
        }
    }

    pub fn loop_state(&self) -> DevLoopState {
        if self.build_error.is_some() { DevLoopState::Failed }
        else if self.build_child.is_some() { DevLoopState::Building }
        else { DevLoopState::Watching }
    }
}
