// Responsibility: Coordinates reload request queues, debouncing, and telemetry recording for the Engine.

use super::registry::{take_build_notes, ComponentRegistry};
use super::telemetry::{HotReloadStats, ReloadReport};
use super::watcher::FileWatcher;
use crate::dsl::element::Element;
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub type SharedFactory = Rc<dyn Fn() -> Element>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReloadKind {
    Full,
    Component(Box<str>),
}

pub struct HotReload {
    registry: ComponentRegistry,
    watcher: FileWatcher,
    root_factory: Option<SharedFactory>,
    last_tree_summary: Option<String>,
    pending: Vec<ReloadKind>,
    last_trigger: FxHashMap<PathBuf, Instant>,
    pub debounce: Duration,
    stats: HotReloadStats,
}

impl HotReload {
    pub fn new() -> Self {
        Self {
            registry: ComponentRegistry::default(),
            watcher: FileWatcher::new(),
            root_factory: None,
            last_tree_summary: None,
            pending: Vec::new(),
            last_trigger: FxHashMap::default(),
            debounce: Duration::from_millis(150),
            stats: HotReloadStats::default(),
        }
    }

    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    pub fn set_root_factory(&mut self, factory: SharedFactory) {
        self.root_factory = Some(factory);
    }

    pub fn root_factory(&self) -> Option<SharedFactory> {
        self.root_factory.clone()
    }

    pub fn queue_full_reload(&mut self) {
        self.pending.push(ReloadKind::Full);
    }

    pub fn queue_component_reload(&mut self, instance: impl AsRef<str>) {
        self.pending.push(ReloadKind::Component(instance.as_ref().into()));
    }

    pub fn last_tree_summary(&self) -> Option<&str> {
        self.last_tree_summary.as_deref()
    }

    pub fn set_last_tree_summary(&mut self, summary: String) {
        self.last_tree_summary = Some(summary);
    }

    pub fn stats(&self) -> &HotReloadStats {
        &self.stats
    }

    pub fn stats_mut(&mut self) -> &mut HotReloadStats {
        &mut self.stats
    }

    pub fn watcher_mut(&mut self) -> &mut FileWatcher {
        &mut self.watcher
    }

    pub fn take_collapsed_pending(&mut self) -> Option<Vec<String>> {
        if self.pending.is_empty() {
            return None;
        }
        let mut full = false;
        let mut targets = Vec::new();
        for req in self.pending.drain(..) {
            match req {
                ReloadKind::Full => full = true,
                ReloadKind::Component(inst) => {
                    let name = inst.to_string();
                    if !targets.contains(&name) {
                        targets.push(name);
                    }
                }
            }
        }
        if full {
            Some(vec!["*".to_string()])
        } else {
            targets.sort();
            Some(targets)
        }
    }

    pub fn drain_build_notes(&mut self) {
        for note in take_build_notes() {
            self.registry.note_built(note.path.into(), note.file);
        }
        self.stats.components_tracked = self.registry.len();
    }

    pub fn drain_watcher(&mut self) -> Vec<PathBuf> {
        let changed = self.watcher.poll_changed();
        if changed.is_empty() {
            return Vec::new();
        }
        let now = Instant::now();
        let mut triggered = Vec::new();
        for path in changed {
            if let Some(last) = self.last_trigger.get(&path) {
                if now.duration_since(*last) < self.debounce {
                    continue;
                }
            }
            self.last_trigger.insert(path.clone(), now);
            triggered.push(path);
            self.queue_full_reload();
        }
        triggered
    }

    pub fn record_report(&mut self, report: &ReloadReport) {
        self.stats.record_report(report);
    }

    pub fn record_build_failure(&mut self, cause: String, log: Vec<String>) {
        self.stats.record_build_failure(cause, log);
    }
}

impl Default for HotReload {
    fn default() -> Self {
        Self::new()
    }
}
