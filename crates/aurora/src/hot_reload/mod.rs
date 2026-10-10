//! State-preserving hot-reload: re-run UI builders, reconcile, keep every signal.
//!
//! Hot-reload answers "I edited a component, now what" without restarting the app:
//!
//! 1. **Detect** — [`FileWatcher`] notices a source file changed (or the debug panel sends
//!    `ReloadAll`, or `F6` is pressed).
//! 2. **Re-execute** — the registered root factory (or a freshly swapped dylib factory via
//!    [`DylibReloader`]) rebuilds the ephemeral [`Element`](crate::dsl::Element) tree inside
//!    the same stable `"root"` hook scope it first built under.
//! 3. **Reconcile** — [`Engine::set_root`](crate::runtime::Engine::set_root) diffs the new
//!    tree against the live arena: unchanged nodes keep their `NodeId`, their layout cache,
//!    and their focus; only what changed repaints.
//! 4. **Preserve** — hook state is keyed by stable component identity, so `use_signal`,
//!    `use_transition`, and tickers survive verbatim. No ticker epoch bump, no hook-state
//!    clear (that amnesia belongs to demo switches, not reloads).
//!
//! # The code-swap story
//!
//! Re-executing the *same* factory rebuilds the *same* tree — that is live-reload (useful
//! for signal-driven content), not hot-reload. True code-swap needs the factory itself to
//! change, and Rust has no stable ABI for swapping `fn` pointers in place. The supported
//! dev-loop shape is therefore:
//!
//! - The UI crate builds as a `cdylib` exposing a build entry point.
//! - The host owns a [`DylibReloader`], calls `refresh()` per frame (or on watcher
//!   events), and on success installs the new factory with
//!   [`HotReload::set_root_factory`] and queues a reload.
//! - Single-binary apps skip the dylib and call [`HotReload::queue_full_reload`] directly
//!   (manual trigger, debug button, `F6`): state-preserving remount with the current code.
//!
//! # Showcase
//!
//! ```ignore
//! use aurora::hot_reload::HotReload;
//!
//! // In the host: rebuild the root from whatever factory is current,
//! // preserving every signal, spring, and ticker.
//! hot_reload.queue_full_reload();
//! ```
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`registry`] | [`registry::ComponentRegistry`]: mounted identities + source-file index |
//! | [`watcher`] | [`watcher::FileWatcher`]: `notify`-backed change polling |
//! | [`component`] | [`component::ComponentReloadExtension`]: real-component code swap |
//! | [`dev_loop`] | [`dev_loop::DevLoop`]: shared watch → build → refresh cycle |
//! | [`dylib`] | [`dylib::DylibReloader`]: code-swap library loading |
//! | [`remote`] | [`remote::RemoteHost`]: data-only remote UI schema + interpreter |
//! | [`extension`] | [`extension::RemoteReloadExtension`]: frame-driven remote dev loop |
//!
//! # Pitfalls
//!
//! - Reload re-runs builders; it does not migrate hook *shapes*. A component whose hook
//!   signature changed (e.g. `use_signal(0)` became `use_signal("x")`) fails loudly with
//!   both types named — fix the hook, don't re-queue.
//! - Anonymous scopes (`Cx::enter` without a key) never preserve state, reloaded or not.
//! - A reload that panics keeps the previous tree and records the error in stats and in
//!   `Engine::last_frame_error` — the app never dies for a bad edit.

pub mod component;
pub mod dev_loop;
pub mod dylib;
pub mod extension;
pub mod registry;
pub mod remote;
pub mod watcher;

pub use component::{ComponentReloadExtension, ComponentRootFn, ComponentVersionFn, COMPONENT_ABI_VERSION};
pub use dev_loop::{DevLoop, DevLoopConfig, PollOutcome};
pub use dylib::DylibReloader;
pub use extension::RemoteReloadExtension;

/// Installs a `tracing_subscriber` fmt logger honoring `RUST_LOG` (default `info`).
///
/// Hot-loop crates (host dylibs) must not depend on `tracing-subscriber` directly: as an
/// extra rlib beside the shared dynamic image it trips rustc's one-format rule for the
/// sysroot closure. Routing init through here keeps their dependency set to exactly one
/// crate. Safe to call twice (only the first call wins).
pub fn init_dev_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().expect("info parses")),
        )
        .with_target(false)
        .try_init();
}
pub use registry::ComponentRegistry;
pub use remote::{
    RemoteBuildFn, RemoteDirection, RemoteEventFn, RemoteHost, RemoteKind, RemoteNode,
    RemoteSize, RemoteSnapshot, RemoteStateDecl, RemoteStateFn, RemoteText, RemoteVersionFn,
    StateWrite, REMOTE_ABI_VERSION,
};
pub use watcher::FileWatcher;

use crate::dsl::element::Element;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A rebuildable UI root: `Rc` so the engine can hold one while the host swaps in another.
pub type SharedFactory = Rc<dyn Fn() -> Element>;

/// What triggered a queued reload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReloadKind {
    /// Rebuild everything from the root factory (state-preserving: hooks and tickers live on).
    Full,
    /// A component identity asked to rebuild; resolves to a full rebuild scoped-reported
    /// under that identity (the reconciler still patches only what changed).
    Component(Box<str>),
}

/// One queued reload request.
#[derive(Clone, Debug)]
struct PendingReload {
    kind: ReloadKind,
}

/// Outcome of one executed reload, for stats and debug-panel feedback.
///
/// `laid_out` / `nodes_dirtied` / `damage_rects` describe what the reload frame actually
/// did (filled in after the frame passes run): a color-only reload repaints without
/// re-laying-out siblings, which is what makes scoped reloads observable rather than
/// merely reported.
#[derive(Clone, Debug)]
pub struct ReloadReport {
    /// Component identities named by the request (`["*"]` for a full reload).
    pub reloaded: Vec<String>,
    /// How long the rebuild + reconcile took (frame passes excluded).
    pub duration: Duration,
    /// `Some` when the rebuild panicked and the previous tree was kept.
    pub error: Option<String>,
    /// Whether the reload frame ran layout.
    pub laid_out: bool,
    /// Nodes dirtied during the reload frame.
    pub nodes_dirtied: usize,
    /// Damaged rects the reload frame produced.
    pub damage_rects: usize,
}

/// Cumulative hot-reload telemetry.
#[derive(Clone, Debug, Default)]
pub struct HotReloadStats {
    /// Successful reloads executed.
    pub reloads_total: u64,
    /// Failed reloads (previous tree kept).
    pub reloads_failed: u64,
    /// Component identities reloaded, in order (may repeat across reloads).
    pub reloaded_log: Vec<String>,
    /// How long the last reload took.
    pub last_duration_ms: f64,
    /// Identities named by the last reload.
    pub last_reloaded: Vec<String>,
    /// Error from the last reload, if it failed.
    pub last_error: Option<String>,
    /// Error from the last failed *build* (cargo exit), if any. Rebuild panics land in
    /// `last_error` instead; this is the compile that never produced a library.
    pub last_build_error: Option<String>,
    /// Stderr tail of the last failed build, newest last. Empty unless a build failed.
    pub last_build_log: Vec<String>,
    /// Whether the last reload's frame ran layout.
    pub last_laid_out: bool,
    /// Nodes dirtied by the last reload's frame.
    pub last_nodes_dirtied: usize,
    /// Damaged rects produced by the last reload's frame.
    pub last_damage_rects: usize,
    /// Mounted component identities known to the registry.
    pub components_tracked: usize,
}

/// Hot-reload orchestrator owned by [`crate::runtime::Engine`].
///
/// Dormant until used: with no factory set and nothing queued, `drain` is a flag check.
/// Enablement is therefore a call (`set_root_factory` / `queue_full_reload`), not a
/// feature flag — release builds carry an empty queue and pay nothing per frame.
pub struct HotReload {
    registry: ComponentRegistry,
    watcher: FileWatcher,
    root_factory: Option<SharedFactory>,
    pending: Vec<PendingReload>,
    /// Last time each path triggered a reload (debounce).
    last_trigger: FxHashMap<PathBuf, Instant>,
    /// Minimum interval between two reloads triggered by the same path.
    pub debounce: Duration,
    stats: HotReloadStats,
}

impl HotReload {
    /// Creates a dormant orchestrator (150 ms same-path debounce).
    pub fn new() -> Self {
        Self {
            registry: ComponentRegistry::default(),
            watcher: FileWatcher::new(),
            root_factory: None,
            pending: Vec::new(),
            last_trigger: FxHashMap::default(),
            debounce: Duration::from_millis(150),
            stats: HotReloadStats::default(),
        }
    }

    /// Installs (or swaps) the root factory re-run on every full reload.
    ///
    /// `AppConfig::run` installs the initial one; a dylib host swaps in a fresh factory
    /// after [`DylibReloader::refresh`] reports a new library.
    pub fn set_root_factory(&mut self, factory: SharedFactory) {
        self.root_factory = Some(factory);
    }

    /// Whether a root factory is installed.
    pub fn has_root_factory(&self) -> bool {
        self.root_factory.is_some()
    }

    /// Queues a full reload: rebuild from the root factory on the next frame.
    pub fn queue_full_reload(&mut self) {
        self.pending.push(PendingReload {
            kind: ReloadKind::Full,
        });
    }

    /// Queues a reload scoped to `instance` (a stable `module_path!()::Name`).
    ///
    /// Unknown identities are reported, not errored: the component may be conditionally
    /// mounted (inside `<if>`) and simply absent right now.
    pub fn queue_component_reload(&mut self, instance: impl AsRef<str>) {
        self.pending.push(PendingReload {
            kind: ReloadKind::Component(instance.as_ref().into()),
        });
    }

    /// Queues reloads for every component expanded from `path`.
    ///
    /// Returns the implicated identities. Empty means the file holds no tracked
    /// components (or nothing built yet) — the caller typically falls back to a full
    /// reload so theme/asset-only edits still apply.
    pub fn queue_reload_for_file(&mut self, path: &Path) -> Vec<String> {
        // Exact registry match first; watcher paths are canonicalized absolute while
        // `file!()` paths are build-relative, so fall back to file-name matching.
        let mut hits: Vec<String> = self
            .registry
            .instances_for_file(path)
            .iter()
            .map(|s| s.to_string())
            .collect();
        if hits.is_empty() {
            hits = self.match_file_loosely(path);
        }
        for inst in &hits {
            self.queue_component_reload(inst);
        }
        hits
    }

    /// Whether any reload is queued.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Number of queued reloads.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Component registry (mounted identities + source-file index).
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    /// Filesystem watcher (register paths with `watcher_mut().watch(...)`).
    pub fn watcher_mut(&mut self) -> &mut FileWatcher {
        &mut self.watcher
    }

    /// Cumulative telemetry.
    pub fn stats(&self) -> &HotReloadStats {
        &self.stats
    }

    /// Collapses the whole queue into one rebuild.
    ///
    /// One rebuild covers every queued request: re-executing the root factory reconciles
    /// the entire tree, so N queued reloads cost one pass. Returns the report targets
    /// (`["*"]` when any full reload was queued, else the named identities).
    pub(crate) fn take_collapsed_pending(&mut self) -> Option<Vec<String>> {
        if self.pending.is_empty() {
            return None;
        }
        let mut full = false;
        let mut targets = Vec::new();
        for req in self.pending.drain(..) {
            match req.kind {
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

    /// Clones the current root factory, if installed.
    pub(crate) fn root_factory(&self) -> Option<SharedFactory> {
        self.root_factory.clone()
    }

    /// Drains thread-local component build notes into the registry.
    pub(crate) fn drain_build_notes(&mut self) {
        for note in registry::take_build_notes() {
            self.registry.note_built(note.path.into(), note.file);
        }
        self.stats.components_tracked = self.registry.len();
    }

    /// Polls the watcher and queues reloads for changed paths (debounced per path).
    ///
    /// Paths with no tracked components still queue one full reload (shared across all
    /// such paths per poll) so asset/theme-only edits apply. Returns implicated files.
    pub(crate) fn drain_watcher(&mut self) -> Vec<PathBuf> {
        let changed = self.watcher.poll_changed();
        if changed.is_empty() {
            return Vec::new();
        }
        let now = Instant::now();
        let mut implicated = Vec::new();
        let mut untracked_full_queued = false;
        // Match watcher output against registered files loosely: compare by file name when
        // the registry holds build-relative paths and the watcher absolute ones.
        for path in changed {
            if let Some(last) = self.last_trigger.get(&path) {
                if now.duration_since(*last) < self.debounce {
                    continue;
                }
            }
            let hits = self.match_file_loosely(&path);
            self.last_trigger.insert(path.clone(), now);
            implicated.push(path.clone());
            if hits.is_empty() {
                if !untracked_full_queued {
                    self.queue_full_reload();
                    untracked_full_queued = true;
                }
            } else {
                for inst in hits {
                    self.queue_component_reload(inst);
                }
            }
        }
        implicated
    }

    /// Records a completed reload in stats.
    pub(crate) fn record_report(&mut self, report: &ReloadReport) {        if report.error.is_some() {
            self.stats.reloads_failed += 1;
        } else {
            self.stats.reloads_total += 1;
        }
        self.stats.last_duration_ms = report.duration.as_secs_f64() * 1000.0;
        self.stats.last_reloaded = report.reloaded.clone();
        self.stats.last_error = report.error.clone();
        self.stats.last_laid_out = report.laid_out;
        self.stats.last_nodes_dirtied = report.nodes_dirtied;
        self.stats.last_damage_rects = report.damage_rects;
        self.stats.reloaded_log.extend(report.reloaded.iter().cloned());
    }

    /// Records a failed *build* (cargo exit) in stats.
    ///
    /// Separate from rebuild panics (`last_error`): the library was never produced, so
    /// the previous UI stays mounted untouched. Read by debug surfaces; the loop itself
    /// waits for the next save rather than retrying.
    pub(crate) fn record_build_failure(&mut self, cause: String, log: Vec<String>) {
        self.stats.last_build_error = Some(cause);
        self.stats.last_build_log = log;
    }

    /// Component identities whose registered source file matches `changed`.
    ///
    /// Exact path match first, then file-name match: the registry holds the literal
    /// `file!()` paths from macro expansion (build-relative) while the watcher hands
    /// back canonicalized absolute paths.
    fn match_file_loosely(&self, changed: &Path) -> Vec<String> {
        let changed_name = changed.file_name();
        let mut out = Vec::new();
        for (file, instances) in self.registry.file_entries() {
            let same = watcher::paths_match(&file, changed)
                || (changed_name.is_some() && file.file_name() == changed_name);
            if same {
                out.extend(instances.iter().map(|s| s.to_string()));
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

impl Default for HotReload {
    fn default() -> Self {
        Self::new()
    }
}
