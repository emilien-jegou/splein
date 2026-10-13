// Single responsibility: Shared watch → background build → library refresh cycle.
//
// Both dev loops (remote data UI and real-component code swap) run the same cycle: watch
// sources, rebuild the UI crate out of the frame loop, load the fresh image. Shared here
// so fixes land once: edits arriving mid-build re-queue instead of being dropped, and a
// fresh on-disk library loads without a redundant initial build.
//
// The loop is poll-driven (`poll` per frame and per idle wake): a build finishing, a
// library changing by hand, and sources changing all surface as [`PollOutcome`]. Symbol
// resolution stays with the caller — this struct hands out the reloader, never symbols.
//
// One build command does everything: `cargo build -p <package>` rebuilds the shared image
// first when stale (same unit graph, so fingerprints agree by construction). The loop
// appends its own `RUSTFLAGS` of `-C prefer-dynamic` (always, to match the hand builds)
// plus `--sysroot` pinned to the toolchain's reported sysroot (skipped on split
// toolchain installations, which expose every sysroot crate twice when pinned —
// either mismatch fails dylib-consuming links with "only shows up once").

use crate::hot_reload::dylib::DylibReloader;
use crate::hot_reload::watcher::FileWatcher;
use std::path::PathBuf;
use std::process::Child;
use std::time::SystemTime;

/// What to build, watch, and load. Shared by every dev-loop flavor.
pub struct DevLoopConfig {
    /// Cargo package whose `cdylib` holds the UI (rebuilt by the loop).
    pub package: String,
    /// Generative-UI example name (`AURORA_HOT_EXAMPLE`): when set, the loop
    /// rebuilds the shared generative crate for this example and watches its
    /// folder. `None` keeps the legacy per-example-package behavior.
    pub example: Option<String>,
    /// Workspace root: build working directory and anchor for default paths.
    pub workspace_root: PathBuf,
    /// Sources watched for changes. Defaults to `crates/<package>/src`.
    pub watch_dir: Option<PathBuf>,
    /// Library image loaded. Defaults to `target/release/lib<package>.so` (Linux-first).
    pub lib_path: Option<PathBuf>,
    /// Rebuild automatically on change (off = react to manual `cargo build` only).
    pub auto_build: bool,
    /// Companion packages co-built with the UI in the SAME `cargo build`
    /// invocation. `spawn_build` sorts and deduplicates companions plus the UI
    /// package into one canonical root set, so every loop — whichever example
    /// it serves — resolves the same unit family. In practice this is just the
    /// shared image alongside the UI (`[shared, package]`); keep it minimal —
    /// every extra package is another closure that can diverge the graph.
    ///
    /// Load-bearing, not decorative: different `-p` root sets resolve
    /// different unit-variant families for the same crates, and linking a
    /// consumer against one variant's shared image while loading another fails
    /// the build ("only shows up once") or the load (undefined symbols).
    /// `example.sh` builds the same set by hand (plus the static launcher,
    /// separately).
    pub companion_packages: Vec<String>,
}

impl DevLoopConfig {
    /// Library image path: explicit override or the default under the workspace root.
    /// The profile segment comes from [`HOT_PROFILE`](crate::hot_reload::HOT_PROFILE),
    /// never a literal: script builds, loop rebuilds, and this default must agree.
    pub fn lib_path(&self) -> PathBuf {
        self.lib_path.clone().unwrap_or_else(|| {
            self.workspace_root.join(format!(
                "target/{}/lib{}.so",
                super::HOT_PROFILE,
                self.package.replace('-', "_")
            ))
        })
    }

    /// Sources watched: explicit override or the package's `src` dir.
    /// With `example` set (generative UI crate), the example's own folder —
    /// its `view.rs`/`hot.rs` are the live sources. Otherwise probes
    /// `crates/<package>/src` first, then `examples/<package>/src`
    /// (all hot-reloadable demos live under `examples/`; the old
    /// crates-only default watched a nonexistent dir and silently
    /// never fired).
    pub fn watch_dir(&self) -> PathBuf {
        if let Some(example) = self.example.as_deref() {
            let dir = self.workspace_root.join("examples").join(example);
            if dir.exists() {
                return dir;
            }
        }
        self.watch_dir.clone().unwrap_or_else(|| {
            let crates = self
                .workspace_root
                .join("crates")
                .join(&self.package)
                .join("src");
            if crates.exists() {
                return crates;
            }
            let examples = self
                .workspace_root
                .join("examples")
                .join(&self.package)
                .join("src");
            if examples.exists() {
                return examples;
            }
            crates
        })
    }

    /// Package manifest watched alongside sources: dependency and feature edits rebuild
    /// through the same cycle as source edits. Missing manifests are simply unwatched.
    /// Probes `crates/<package>/Cargo.toml` first, then `examples/<package>/Cargo.toml`.
    pub fn manifest_path(&self) -> PathBuf {
        let crates = self
            .workspace_root
            .join("crates")
            .join(&self.package)
            .join("Cargo.toml");
        if crates.exists() {
            return crates;
        }
        let examples = self
            .workspace_root
            .join("examples")
            .join(&self.package)
            .join("Cargo.toml");
        if examples.exists() {
            return examples;
        }
        crates
    }
}

/// What one `poll` observed, in priority order.
#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// A fresh library image is live (built by us or by hand) — resolve and swap.
    LibraryReady,
    /// A build finished but the library did not change (or failed) — stay put.
    Settled,
    /// A build is running — keep waking the loop until it completes.
    Building,
    /// Nothing changed and nothing runs.
    Idle,
}

/// How often a poll may walk source mtimes for the freshness backstop. The watcher is
/// the fast path; this walk only needs to catch what the watcher missed.
const FRESHNESS_WALK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// What the dev loop is doing, for stats/HUD/panel display.
///
/// Previously a stuck loop was inferable only from the *absence* of log lines; this
/// makes every state — especially `Stale` (sources newer than image, no build running)
/// — queryable through `HotReloadStats`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevLoopState {
    /// Idle and fresh: watching, image matches sources.
    #[default]
    Watching,
    /// Sources are newer than the loaded image with no build running (auto-build off,
    /// spawn failed, or the backstop hasn't fired yet).
    Stale,
    /// A build is running.
    Building,
    /// The last build failed; waiting for the next save (see `last_build_error`).
    Failed,
}

/// Watch → build → refresh cycle shared by the dev loops.
///
/// The loop is state-convergent, not edge-triggered: besides watcher events it
/// compares source freshness against the loaded image, so a save the watcher missed
/// (before startup, during a restart, dropped event) still rebuilds instead of serving
/// a stale image forever.
pub struct DevLoop {
    config: DevLoopConfig,
    loader: DylibReloader,
    watcher: FileWatcher,
    build_child: Option<Child>,
    /// Cargo log of the running/finished build (stderr). Overwritten per spawn; read as
    /// a tail on failure so a broken edit explains itself through stats instead of
    /// requiring a terminal.
    build_log: PathBuf,
    /// Last failed build waiting to be reported: one-line cause plus stderr tail.
    /// Set on failure, consumed once by `take_build_error`.
    build_error: Option<(String, Vec<String>)>,
    /// Exact source closure the compiler consumed (cargo depinfo), refreshed after every
    /// load. Watching this instead of just the package `src/` dir keeps `#[path]`
    /// includes from elsewhere visible to both the watcher and the freshness walk.
    /// Empty until the first depinfo parse — the watch-dir walk covers the gap.
    source_files: Vec<PathBuf>,
    /// Newest source mtime as of the last build spawn, from any cause. The freshness
    /// backstop only fires on a *new* maximum, so a failed build (image still stale)
    /// never retriggers itself into a rebuild loop.
    last_trigger_sources: Option<SystemTime>,
    /// Newest source mtime as of the last walk, for [`DevLoop::loop_state`] without
    /// re-walking every frame.
    last_max_sources: Option<SystemTime>,
    /// Last freshness walk, for throttling.
    last_freshness_check: std::time::Instant,
    /// Sources changed while a build was running: rebuild again when it completes.
    ///
    /// Without this, saves landing in the (long) link window are drained from the watcher
    /// and forgotten — the window shows stale UI until the *next* save for no reason.
    dirty_during_build: bool,
}

impl DevLoop {
    /// Creates the loop: watches sources (the package `src/` dir until the first
    /// depinfo parse narrows it to the exact source closure). Nothing loads or builds
    /// here — the first `poll` activates any on-disk library, spawns the initial build
    /// when nothing is loadable, or rebuilds when sources are newer than the image.
    pub fn new(config: DevLoopConfig) -> Self {
        let lib_path = config.lib_path();
        let watch_dir = config.watch_dir();
        let mut watcher = FileWatcher::new();
        watcher.watch(watch_dir.clone());
        // Manifest edits (deps, features) rebuild like source edits; a missing manifest
        // is inert (watch logs and ignores unwatched paths).
        watcher.watch(config.manifest_path());
        tracing::info!(
            "aurora: hot-reload watching {} (package `{}`)",
            watch_dir.display(),
            config.package
        );
        let build_log =
            std::env::temp_dir().join(format!("aurora-hot-build-{}.log", std::process::id()));
        Self {
            loader: DylibReloader::new(lib_path),
            watcher,
            build_child: None,
            build_log,
            build_error: None,
            source_files: Vec::new(),
            last_trigger_sources: None,
            last_max_sources: None,
            // Expired, so the first poll walks freshness immediately instead of
            // waiting out the throttle.
            last_freshness_check: std::time::Instant::now() - FRESHNESS_WALK_INTERVAL,
            dirty_during_build: false,
            config,
        }
    }

    /// The package being looped.
    pub fn package(&self) -> &str {
        &self.config.package
    }

    /// The library reloader (resolve symbols from it on [`PollOutcome::LibraryReady`]).
    pub fn loader(&self) -> &DylibReloader {
        &self.loader
    }

    /// Mutable reloader access (tests driving manual refreshes).
    pub fn loader_mut(&mut self) -> &mut DylibReloader {
        &mut self.loader
    }

    /// Whether a background build is running.
    pub fn is_building(&self) -> bool {
        self.build_child.is_some()
    }

    /// Takes the last failed build's summary, if one is waiting.
    ///
    /// Consuming: each failure reports exactly once, to exactly one poller. Returns the
    /// one-line cause plus the captured stderr tail (newest last, capped).
    pub fn take_build_error(&mut self) -> Option<(String, Vec<String>)> {
        self.build_error.take()
    }

    /// Shared idle lap for the dev-loop extensions (component + remote).
    ///
    /// An idle app presents no frames, so frame polling alone would sleep through
    /// edits. A `LibraryReady` observed here cannot activate (no engine access) — it
    /// latches `pending_activation` so the next update lap swaps instead of dropping
    /// the observation.
    pub fn poll_idle(&mut self, pending_activation: &mut bool) -> bool {
        if *pending_activation {
            return true;
        }
        match self.poll() {
            PollOutcome::LibraryReady => {
                *pending_activation = true;
                true
            }
            PollOutcome::Building | PollOutcome::Settled => true,
            PollOutcome::Idle => false,
        }
    }

    /// Shared update lap for the dev-loop extensions.
    ///
    /// Runs the latched activation first (an idle poll may have observed a fresh
    /// library), then the flavor's `interact` hook (remote drains click actions;
    /// component passes none), then the dev-cycle poll with activation on
    /// `LibraryReady`, and finally reports one waiting build failure into the engine.
    /// Returns whether the engine should present another frame. `activate` receives
    /// the loop (for symbol resolution) and the engine; both flavors' activation
    /// signatures fit because they only borrow disjoint extension state.
    pub fn poll_update(
        &mut self,
        engine: &mut crate::runtime::Engine,
        pending_activation: &mut bool,
        activate: &mut dyn FnMut(&mut DevLoop, &mut crate::runtime::Engine) -> bool,
        interact: &mut dyn FnMut() -> bool,
    ) -> bool {
        let mut redraw = false;
        if *pending_activation {
            *pending_activation = false;
            redraw |= activate(self, engine);
        }
        redraw |= interact();
        match self.poll() {
            PollOutcome::LibraryReady => redraw |= activate(self, engine),
            PollOutcome::Building | PollOutcome::Settled => redraw = true,
            PollOutcome::Idle => {}
        }
        // A failed build explains itself through stats (HUD/panel read these); the loop
        // itself waits for the next save rather than retrying blindly.
        if let Some((cause, log)) = self.take_build_error() {
            engine.hot_reload_mut().record_build_failure(cause, log);
            redraw = true;
        }
        // Publish the loop state every lap so a stuck loop is visible in stats,
        // not inferable from missing log lines.
        let state = self.loop_state();
        engine.hot_reload_mut().set_dev_state(state);
        redraw
    }

    /// Advances the cycle: polls sources and the background build.
    ///
    /// Call once per frame and once per idle wake (an idle app presents no frames, so
    /// frame polling alone would sleep through edits — see the `on_idle` wiring in the
    /// extensions).
    ///
    /// State-convergent: besides watcher events, a throttled freshness walk rebuilds
    /// whenever sources are newer than the loaded image — including saves that landed
    /// before startup or while restarting, which no watcher ever saw.
    pub fn poll(&mut self) -> PollOutcome {
        let watcher_hit = !self.watcher.poll_changed().is_empty();
        if self.build_child.is_some() {
            if watcher_hit {
                self.dirty_during_build = true;
            }
            return self.poll_build();
        }
        let mut trigger = watcher_hit;
        let mut reason = "sources changed";
        if !trigger && self.freshness_due() {
            // Backstop only (see `decide_freshness`): fires on a *new* source maximum
            // older than nothing we already built for, and only when the loaded image
            // is actually older — so failed builds (image still stale) never
            // retrigger themselves into a rebuild loop.
            if let Some(max_src) = self.max_source_mtime() {
                let (decision, baseline) = decide_freshness(
                    max_src,
                    self.last_trigger_sources,
                    self.loader.loaded_mtime(),
                );
                self.last_trigger_sources = baseline;
                if decision == FreshDecision::Build {
                    trigger = true;
                    reason = "sources newer than image";
                }
            }
            self.last_freshness_check = std::time::Instant::now();
        }
        if trigger {
            // Baseline the trigger so the freshness walk agrees a build is in flight
            // for exactly these sources (the walk itself is throttled anyway).
            if self.max_source_mtime().is_some() {
                self.last_trigger_sources = self.max_source_mtime();
            }
            self.spawn_build(reason);
            if self.build_child.is_some() {
                return PollOutcome::Building;
            }
            // Auto-build off: no build follows, but the watcher did fire — report
            // `Building` so the loop keeps waking (a build *would* follow in a live
            // config). A failed spawn instead falls through to the hand-built check
            // rather than spinning `Building` forever.
            if !self.config.auto_build {
                return PollOutcome::Building;
            }
        }
        // No build running: pick up fresh libraries (built by us, or by hand) and —
        // on the very first polls — activate whatever is already on disk.
        if self.loader.refresh() {
            self.refresh_source_set();
            return PollOutcome::LibraryReady;
        }
        // Nothing loadable yet and no build running: the initial build.
        if !self.loader.is_loaded() {
            self.spawn_build("initial");
            if self.build_child.is_some() {
                return PollOutcome::Building;
            }
        }
        PollOutcome::Idle
    }

    /// Whether the throttled freshness walk is due.
    fn freshness_due(&self) -> bool {
        self.last_freshness_check.elapsed() >= FRESHNESS_WALK_INTERVAL
    }

    /// Newest mtime over the known source set: the depinfo closure once parsed, else a
    /// recursive walk of the watched package dir — plus the watched manifest either
    /// way. Missing/unreadable files are skipped, never fatal — worst case the
    /// backstop stays quiet and the watcher decides alone. Records the result for
    /// [`DevLoop::loop_state`].
    fn max_source_mtime(&mut self) -> Option<SystemTime> {
        let mut max = if self.source_files.is_empty() {
            max_mtime_under(&self.config.watch_dir())
        } else {
            let mut max: Option<SystemTime> = None;
            for file in &self.source_files {
                let Ok(mtime) = std::fs::metadata(file).and_then(|m| m.modified()) else {
                    continue;
                };
                max = Some(max.map_or(mtime, |t: SystemTime| t.max(mtime)));
            }
            max
        };
        // The manifest is watched but rarely inside depinfo: always include it.
        let manifest = self.config.manifest_path();
        if !self.source_files.iter().any(|f| f == &manifest) {
            if let Ok(mtime) = std::fs::metadata(&manifest).and_then(|m| m.modified()) {
                max = Some(max.map_or(mtime, |t: SystemTime| t.max(mtime)));
            }
        }
        self.last_max_sources = max;
        max
    }

    /// What the loop is doing right now, without walking anything. See [`DevLoopState`].
    ///
    /// `Stale` means sources are newer than the loaded image *and* newer than
    /// anything already built for: a successful build that produced no new
    /// image (no-op save — cargo found nothing to do) converges back to
    /// `Watching`, since the image already reflects those sources. Without the
    /// trigger baseline, every no-diff save would read `Stale` forever and pin
    /// the progress bar on screen.
    pub fn loop_state(&self) -> DevLoopState {
        if self.build_error.is_some() {
            DevLoopState::Failed
        } else if self.build_child.is_some() {
            DevLoopState::Building
        } else {
            match (self.last_max_sources, self.loader.loaded_mtime()) {
                (Some(s), Some(l)) if s > l => {
                    if self.last_trigger_sources.is_some_and(|t| s <= t) {
                        DevLoopState::Watching
                    } else {
                        DevLoopState::Stale
                    }
                }
                (Some(_), None) => DevLoopState::Stale,
                _ => DevLoopState::Watching,
            }
        }
    }

    /// Re-reads the compiler's depinfo for the UI crate and watches exactly the source
    /// closure it lists — wherever `#[path]` includes actually live. Falls back to the
    /// package `src/` dir when no depinfo exists yet (never built here).
    fn refresh_source_set(&mut self) {
        let depinfo = self.config.lib_path().with_extension("d");
        let Ok(text) = std::fs::read_to_string(&depinfo) else {
            return;
        };
        let mut files = parse_depinfo(&text);
        if files.is_empty() {
            return;
        }
        files.sort();
        files.dedup();
        // Only adopt a set that still contains the manifest dir's crate sources would be
        // over-strict (depinfo also lists registry sources); adopt whatever parsed.
        for file in &files {
            self.watcher.watch(file.clone());
        }
        self.source_files = files;
    }

    /// Starts a background build of the canonical hot-loop set unless one is
    /// already running.
    ///
    /// One command builds the whole set (companions plus the UI package, sorted
    /// and deduplicated): a single unit graph means fingerprints agree by
    /// construction. This is load-bearing, not style: different `-p` root sets
    /// resolve different unit-variant families for the same crates (multiple
    /// coexisting `aurora`/`bon` unit hashes with identical features/flags
    /// observed in `target/debug/build/*`), and a consumer linked against one
    /// variant's shared image while the launcher loads another fails with
    /// "only shows up once" or undefined symbols at load. EVERY cargo
    /// invocation that touches these packages — script, loop spawn, tests —
    /// must use this exact set; any deviation spawns a rival family. The build
    /// pins `--sysroot` to the toolchain's own reported sysroot (see
    /// `rustc_sysroot`): split toolchain installations expose the same sysroot
    /// crates twice, and dylib-consuming links fail with "only shows up
    /// once" unless exactly one is visible. The user's own `RUSTFLAGS` are preserved
    /// (appended to, never replaced). No other per-build flags, ever.
    fn spawn_build(&mut self, reason: &str) {
        if !self.config.auto_build || self.build_child.is_some() {
            return;
        }
        let log_file = match std::fs::File::create(&self.build_log) {
            Ok(file) => file,
            Err(e) => {
                tracing::warn!("aurora: hot-reload cannot log build ({e})");
                return;
            }
        };
        // Canonical set, normalized: companions plus the UI package, sorted
        // and deduplicated, so every spawn — whichever loop runs it — resolves
        // the same unit family. Never narrow this per loop.
        let mut command = hot_build_command(&self.config);
        command
            .current_dir(&self.config.workspace_root)
            .stdout(std::process::Stdio::null())
            .stderr(log_file);
        match command.spawn() {
            Ok(child) => {
                tracing::info!(
                    "aurora: hot-reload rebuilding `{}` ({reason}); log: {}",
                    self.config.package,
                    self.build_log.display()
                );
                self.build_child = Some(child);
            }
            Err(e) => {
                tracing::warn!("aurora: hot-reload could not spawn cargo ({e})");
            }
        }
    }
}

/// The one canonical hot-loop build command: `cargo build` over the shared
/// image plus the UI package in a single unit graph (see the load-bearing note
/// on `DevLoopConfig::companion_packages`). Script hand-builds, loop-spawned
/// rebuilds, and the host's blocking boot build ALL go through here, so the
/// shared image and its consumer can never resolve rival unit families.
///
/// Single profile everywhere (`HOT_PROFILE`): runtime speed for interactive
/// demos, one unit family for dylib links. `-C prefer-dynamic` unconditional
/// (matches the hand builds); `--sysroot` pin conditional (split toolchains
/// expose every sysroot crate twice when pinned). No other per-build flags,
/// ever — and never set `CARGO_INCREMENTAL`: toggling incremental state
/// across builds of the same tree poisons dylib relinks ("only shows up
/// once"), because the flag does not version the unit graph.
fn hot_build_command(config: &DevLoopConfig) -> std::process::Command {
    let mut roots: Vec<&str> = config
        .companion_packages
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(config.package.as_str()))
        .collect();
    roots.sort();
    roots.dedup();
    let mut command = std::process::Command::new("cargo");
    command
        .arg("build")
        .arg("--profile")
        .arg(super::HOT_PROFILE);
    for root in &roots {
        command.arg("-p").arg(root);
    }
    let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    if !rustflags.is_empty() {
        rustflags.push(' ');
    }
    rustflags.push_str("-C prefer-dynamic");
    match rustc_sysroot() {
        Some(sysroot) => {
            rustflags.push_str(" --sysroot ");
            rustflags.push_str(&sysroot.to_string_lossy());
        }
        None => {
            tracing::info!(
                "aurora: hot-reload building without a --sysroot pin ({})",
                sysroot_skip_reason()
            );
        }
    }
    command.env("RUSTFLAGS", rustflags);
    // Generative UI crate: the example selects which `view.rs`/`hot.rs`
    // the build script mounts. Forwarded explicitly (not merely inherited)
    // so every build agrees even when the parent process env differs.
    if let Some(example) = config.example.as_deref() {
        command.env("AURORA_HOT_EXAMPLE", example);
    }
    command
}

/// Runs the canonical hot-loop build to completion in the calling process:
/// the host's boot path builds the first UI image *before* creating the
/// window, so the window opens once at its declared size instead of popping
/// from a boot default. Returns the stderr tail on failure for a loud exit.
pub fn blocking_hot_build(
    config: &DevLoopConfig,
    log_path: &std::path::Path,
) -> Result<(), Vec<String>> {
    let log_file = std::fs::File::create(log_path).map_err(|e| {
        vec![format!(
            "aurora: hot-reload cannot log boot build ({}): {e}",
            log_path.display()
        )]
    })?;
    let status = hot_build_command(config)
        .current_dir(&config.workspace_root)
        .stdout(std::process::Stdio::null())
        .stderr(log_file)
        .status()
        .map_err(|e| vec![format!("aurora: hot-reload could not spawn boot build: {e}")])?;
    if status.success() {
        Ok(())
    } else {
        Err(read_tail(log_path, 20))
    }
}

impl DevLoop {
    /// Polls the background build, refreshing the library on success.
    fn poll_build(&mut self) -> PollOutcome {
        let done = match self.build_child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => Some(status.success()),
                Ok(None) => return PollOutcome::Building,
                Err(e) => {
                    tracing::warn!("aurora: hot-reload build poll failed ({e})");
                    Some(false)
                }
            },
            None => return PollOutcome::Idle,
        };
        self.build_child = None;
        if !done.unwrap_or(false) {
            let log = read_tail(&self.build_log, 20);
            let cause = format!("cargo build -p {} failed", self.config.package);
            tracing::warn!("aurora: hot-reload {cause}; keeping previous UI");
            for line in &log {
                tracing::warn!("aurora: hot-reload build | {line}");
            }
            self.build_error = Some((cause, log));
            self.dirty_during_build = false;
            return PollOutcome::Settled;
        }
        // Sources changed mid-build: the image just loaded is already stale — go again
        // immediately instead of waiting for the next save.
        if self.dirty_during_build {
            self.dirty_during_build = false;
            self.spawn_build("sources changed during build");
            return PollOutcome::Building;
        }
        if self.loader.refresh() {
            PollOutcome::LibraryReady
        } else {
            PollOutcome::Settled
        }
    }
}

/// What the freshness backstop decided for one source maximum.
#[derive(Debug, PartialEq, Eq)]
enum FreshDecision {
    /// Sources are newer than anything built for *and* newer than the loaded image:
    /// spawn a build.
    Build,
    /// Up to date, already building for these sources, or image newer (clock skew):
    /// stay put.
    Quiet,
}

/// Pure freshness decision, unit-tested below.
///
/// Fires only on a *new* source maximum (`last_trigger` baselines every spawn from any
/// cause), and only when a loaded image is actually older — so a failed build, which
/// leaves the image stale, never retriggers itself. With no image loaded there is
/// nothing to be stale against: quiet (the `!is_loaded → spawn initial` path in
/// `poll` covers auto-build startup; auto-build-off loops wait for hand-built images
/// via `refresh`). Returns the decision plus the new baseline (advanced even when
/// quiet, so one walk settles the state).
fn decide_freshness(
    max_src: SystemTime,
    last_trigger: Option<SystemTime>,
    loaded: Option<SystemTime>,
) -> (FreshDecision, Option<SystemTime>) {
    if last_trigger.is_some_and(|t| max_src <= t) {
        return (FreshDecision::Quiet, last_trigger);
    }
    let baseline = Some(max_src);
    let stale = loaded.is_some_and(|l| max_src > l);
    let decision = if stale {
        FreshDecision::Build
    } else {
        FreshDecision::Quiet
    };
    (decision, baseline)
}

/// Parses a cargo depinfo (`.d`) file into its dependency paths.
///
/// Make-style `targets: deps...` with backslash-newline continuations and
/// backslash-escaped spaces; target tokens (trailing `:`) are dropped. Only paths that
/// exist on disk are returned — a stale depinfo naming moved files must not add
/// unwatched noise (the watcher logs every path it cannot watch).
fn parse_depinfo(text: &str) -> Vec<PathBuf> {
    let collapsed = text.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut files = Vec::new();
    let mut token = String::new();
    let mut chars = collapsed.chars().peekable();
    let flush = |token: &mut String, files: &mut Vec<PathBuf>| {
        if token.is_empty() || token == "\\" {
            token.clear();
            return;
        }
        if !token.ends_with(':') {
            let path = PathBuf::from(token.as_str());
            if path.exists() && !files.contains(&path) {
                files.push(path);
            }
        }
        token.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Escaped char (usually a space) belongs to the token; a lone
                // backslash (line-continuation remnant) terminates it.
                match chars.peek() {
                    Some(&next) if next != '\r' && next != '\n' => {
                        token.push(next);
                        chars.next();
                    }
                    _ => flush(&mut token, &mut files),
                }
            }
            c if c.is_whitespace() => flush(&mut token, &mut files),
            _ => token.push(c),
        }
    }
    flush(&mut token, &mut files);
    files
}

/// Newest mtime under `dir`, recursive. Missing/unreadable subtrees are skipped, never
/// fatal.
fn max_mtime_under(dir: &std::path::Path) -> Option<SystemTime> {
    fn walk(dir: &std::path::Path, max: &mut Option<SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, max);
            } else if let Ok(mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                *max = Some(max.map_or(mtime, |t: SystemTime| t.max(mtime)));
            }
        }
    }
    let mut max = None;
    walk(dir, &mut max);
    max
}

/// Reads the last `n` lines of a file, newest last. Missing/unreadable files yield
/// nothing rather than an error: a missing log means the build never started.
fn read_tail(path: &std::path::Path, n: usize) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(n);
    lines[skip..].iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Freshness fires only on a new source maximum against an older image — and a
    /// failed build (image still stale, baseline already advanced) never retriggers.
    #[test]
    fn freshness_decision_converges() {
        let t0 = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let t1 = t0 + std::time::Duration::from_secs(10);
        // Stale image, unseen sources: build, baseline advances.
        assert_eq!(
            decide_freshness(t1, None, Some(t0)),
            (FreshDecision::Build, Some(t1))
        );
        // Same maximum again (e.g. build failed, image still stale): quiet.
        assert_eq!(
            decide_freshness(t1, Some(t1), Some(t0)),
            (FreshDecision::Quiet, Some(t1))
        );
        // No image at all: quiet (initial-build and hand-built paths decide).
        assert_eq!(
            decide_freshness(t1, None, None),
            (FreshDecision::Quiet, Some(t1))
        );
        // Image newer than sources (clock skew, hand-placed image): quiet, but the
        // baseline still advances so one walk settles the state.
        assert_eq!(
            decide_freshness(t0, None, Some(t1)),
            (FreshDecision::Quiet, Some(t0))
        );
        // Older sources than the last spawn: quiet.
        assert_eq!(
            decide_freshness(t0, Some(t1), Some(t0)),
            (FreshDecision::Quiet, Some(t1))
        );
    }

    /// Depinfo parsing keeps real deps (including `#[path]` includes outside the crate
    /// dir), drops targets, and tolerates continuations and escaped spaces.
    #[test]
    fn depinfo_parse_keeps_existing_deps() {
        let dir = std::env::temp_dir().join(format!(
            "aurora-hot-depinfo-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let src = dir.join("src");
        std::fs::create_dir_all(&src).expect("temp dirs");
        let lib = src.join("lib.rs");
        let view = dir.join("elsewhere.rs");
        let spaced = src.join("my mod.rs");
        std::fs::write(&lib, "").expect("seed");
        std::fs::write(&view, "").expect("seed");
        std::fs::write(&spaced, "").expect("seed");
        let text = format!(
            "/out/libui.rlib: {} \\\n {} {} /nonexistent/gone.rs",
            lib.display(),
            view.display(),
            spaced.display().to_string().replace(' ', "\\ ")
        );
        let files = parse_depinfo(&text);
        assert!(files.contains(&lib), "crate source kept: {files:?}");
        assert!(files.contains(&view), "`#[path]` include kept: {files:?}");
        assert!(files.contains(&spaced), "escaped space kept: {files:?}");
        assert!(
            !files.iter().any(|f| f.ends_with("gone.rs")),
            "missing files dropped: {files:?}"
        );
        assert!(
            !files.iter().any(|f| f.extension().is_none()),
            "target token dropped: {files:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The loop publishes `Stale` when sources exist but no image is loaded — the
    /// previously invisible "watching, nothing will ever happen" state.
    #[test]
    fn loop_state_reports_stale_without_image() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("aurora-hot-state-{stamp}"));
        let src = root.join("crates").join("pkg").join("src");
        std::fs::create_dir_all(&src).expect("temp workspace");
        std::fs::write(src.join("lib.rs"), "").expect("seed source");
        let mut dev = DevLoop::new(DevLoopConfig {
        example: None,
            package: "pkg".to_string(),
            workspace_root: root.clone(),
            watch_dir: None,
            lib_path: None,
            auto_build: false,
            companion_packages: vec![],
        });
        assert_eq!(dev.loop_state(), DevLoopState::Watching);
        // One poll walks freshness (throttle pre-expired) and records the sources.
        assert_eq!(dev.poll(), PollOutcome::Idle);
        assert_eq!(
            dev.loop_state(),
            DevLoopState::Stale,
            "sources without an image must show, not hide"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The source-set walk reports the newest mtime and skips unreadable subtrees.
    #[test]
    fn source_walk_reports_newest_mtime() {
        let dir = std::env::temp_dir().join(format!(
            "aurora-hot-walk-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).expect("temp dirs");
        let old = dir.join("old.rs");
        let new = sub.join("new.rs");
        std::fs::write(&old, "").expect("seed");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&new, "").expect("seed");
        let max = max_mtime_under(&dir).expect("walk finds files");
        let new_mtime = std::fs::metadata(&new)
            .and_then(|m| m.modified())
            .expect("mtime");
        assert_eq!(max, new_mtime);
        assert!(max_mtime_under(&dir.join("absent")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_reads_newest_last_and_caps() {
        let dir = std::env::temp_dir().join("aurora-hot-tail");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("build.log");
        let body: String = (0..30).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&file, body).expect("seed log");
        let tail = read_tail(&file, 20);
        assert_eq!(tail.len(), 20);
        assert_eq!(tail.first().unwrap(), "line 10");
        assert_eq!(tail.last().unwrap(), "line 29");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_of_missing_file_is_empty() {
        let missing: PathBuf = std::env::temp_dir().join("aurora-hot-nope/missing.log");
        assert!(read_tail(&missing, 20).is_empty());
    }

    /// A failing package surfaces exactly one consumable error with log content.
    ///
    /// Uses a nonexistent package so cargo fails fast at resolution (no compilation,
    /// no waiting on links): the point is the failure plumbing, not a real breakage.
    #[test]
    fn failed_build_reports_once_with_log() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let mut dev = DevLoop::new(DevLoopConfig {
        example: None,
            package: "aurora-hot-nonexistent-xyz".to_string(),
            workspace_root: root,
            watch_dir: None,
            lib_path: None,
            auto_build: true,
            companion_packages: vec![],
        });
        // The initial spawn targets nothing buildable; drive to completion.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut settled = false;
        while std::time::Instant::now() < deadline {
            match dev.poll() {
                PollOutcome::Settled if !dev.is_building() => {
                    settled = true;
                    break;
                }
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(settled, "failing build must settle, not spin");
        let first = dev.take_build_error();
        assert!(first.is_some(), "one error waits");
        let (cause, log) = first.unwrap();
        assert!(cause.contains("aurora-hot-nonexistent-xyz"), "{cause}");
        assert!(!log.is_empty(), "stderr tail captured, not dropped");
        assert!(
            dev.take_build_error().is_none(),
            "consumed errors do not repeat"
        );
    }

    /// Manifest edits trigger the cycle like source edits do.
    ///
    /// Uses a temp workspace (never the real tree): `auto_build = false` so nothing
    /// spawns — detection alone is what this pins (a non-`Idle` poll means the watcher
    /// fired and a build would follow).
    #[test]
    fn manifest_changes_are_observed() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("aurora-hot-manifest-{stamp}"));
        let src = root.join("crates").join("pkg").join("src");
        std::fs::create_dir_all(&src).expect("temp workspace");
        let manifest = root.join("crates").join("pkg").join("Cargo.toml");
        std::fs::write(&manifest, "[package]\nname = \"pkg\"\n").expect("seed manifest");

        let mut dev = DevLoop::new(DevLoopConfig {
        example: None,
            package: "pkg".to_string(),
            workspace_root: root.clone(),
            watch_dir: None,
            lib_path: None,
            auto_build: false,
            companion_packages: vec![],
        });
        // Arm the watcher backend before touching anything.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(dev.poll(), PollOutcome::Idle, "quiet tree stays idle");

        std::fs::write(&manifest, "[package]\nname = \"pkg\"\n# touched\n")
            .expect("touch manifest");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut observed = PollOutcome::Idle;
        while std::time::Instant::now() < deadline {
            observed = dev.poll();
            if observed != PollOutcome::Idle {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = std::fs::remove_dir_all(&root);
        assert_ne!(
            observed,
            PollOutcome::Idle,
            "manifest edit must wake the loop"
        );
    }
}

/// Reports the toolchain's own sysroot (`rustc --print sysroot`), if rustc runs AND
/// pinning it is safe.
///
/// Used to pin `--sysroot` on loop-spawned builds (see `spawn_build`). Resolved from
/// `$RUSTC` when set, else `PATH` — the same rustc cargo would use.
///
/// The pin is skipped (returning `None`) when the sysroot is already composed from
/// parts: if `libstd` canonicalizes *outside* the reported sysroot (Nix-style split
/// installation, unified by the wrapper via symlinks), an explicit `--sysroot`
/// re-exposes the parts and every sysroot crate shows up twice at dylib link time
/// ("only shows up once", observed not theorized). In that case the wrapper's own
/// sysroot handling is authoritative and must not be overridden.
fn rustc_sysroot() -> Option<PathBuf> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = std::process::Command::new(rustc)
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sysroot = PathBuf::from(trimmed);
    if sysroot_is_split(&sysroot) {
        return None;
    }
    Some(sysroot)
}

/// Whether `sysroot` is composed from parts living elsewhere.
///
/// Probes `libstd` (rlib or dylib): a self-contained installation keeps it inside the
/// sysroot, while a split one (Nix) symlinks it in from another store path. Only the
/// canonical location matters — the file name itself varies by toolchain hash.
fn sysroot_is_split(sysroot: &std::path::Path) -> bool {
    let rustlib = sysroot.join("lib").join("rustlib");
    let Ok(targets) = std::fs::read_dir(&rustlib) else {
        return false;
    };
    for target in targets.flatten() {
        let lib_dir = target.path().join("lib");
        let Ok(entries) = std::fs::read_dir(&lib_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if (name.starts_with("libstd.") || name.starts_with("libstd-"))
                && (name.ends_with(".rlib") || name.ends_with(".so"))
            {
                // A dangling or unresolvable probe is inconclusive, not split.
                if let Ok(canonical) = std::fs::canonicalize(entry.path()) {
                    return !canonical.starts_with(sysroot);
                }
            }
        }
    }
    false
}

/// Why no `--sysroot` pin was applied, for the skip log line.
fn sysroot_skip_reason() -> &'static str {
    "split sysroot installation (std resolves outside it); deferring to the toolchain wrapper"
}

#[cfg(test)]
mod sysroot_tests {
    /// A sysroot whose `libstd` canonicalizes outside itself is split (Nix-style):
    /// pinning `--sysroot` there duplicates every sysroot crate at dylib link time.
    #[test]
    fn split_sysroot_detected_by_escaping_std() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("aurora-hot-sysroot-{stamp}"));
        let elsewhere = root.join("elsewhere");
        let lib_dir = root
            .join("sysroot")
            .join("lib")
            .join("rustlib")
            .join("x86_64-unknown-linux-gnu")
            .join("lib");
        std::fs::create_dir_all(&elsewhere).expect("temp dirs");
        std::fs::create_dir_all(&lib_dir).expect("temp dirs");
        let real_std = elsewhere.join("libstd-hash.rlib");
        std::fs::write(&real_std, "fake").expect("seed std");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_std, lib_dir.join("libstd-hash.rlib"))
            .expect("symlink std");
        assert!(
            super::sysroot_is_split(&root.join("sysroot")),
            "escaping libstd must read as split"
        );
        // A self-contained sysroot (real file inside) is not split.
        let plain = root
            .join("plain")
            .join("lib")
            .join("rustlib")
            .join("t")
            .join("lib");
        std::fs::create_dir_all(&plain).expect("temp dirs");
        std::fs::write(plain.join("libstd-hash.rlib"), "fake").expect("seed std");
        assert!(
            !super::sysroot_is_split(&root.join("plain")),
            "contained libstd must not read as split"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sysroot_discovery_points_at_a_sysroot() {
        let Some(sysroot) = super::rustc_sysroot() else {
            // Split installations (and missing rustc) skip the pin: nothing to point at.
            return;
        };
        assert!(
            sysroot.join("lib/rustlib").exists() || sysroot.join("lib").exists(),
            "discovered sysroot looks like a sysroot: {}",
            sysroot.display()
        );
    }
}
