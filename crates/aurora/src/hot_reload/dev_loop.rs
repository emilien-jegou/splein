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
// never sets RUSTFLAGS or otherwise perturbs the build environment — exotic per-build
// flags multiply unit hashes and produce exactly the mixed-invocation skew this loop is
// designed never to see. Keep hand builds vanilla too.

use crate::hot_reload::dylib::DylibReloader;
use crate::hot_reload::watcher::FileWatcher;
use std::path::PathBuf;
use std::process::Child;

/// What to build, watch, and load. Shared by every dev-loop flavor.
pub struct DevLoopConfig {
    /// Cargo package whose `cdylib` holds the UI (rebuilt by the loop).
    pub package: String,
    /// Workspace root: build working directory and anchor for default paths.
    pub workspace_root: PathBuf,
    /// Sources watched for changes. Defaults to `crates/<package>/src`.
    pub watch_dir: Option<PathBuf>,
    /// Library image loaded. Defaults to `target/debug/lib<package>.so` (Linux-first).
    pub lib_path: Option<PathBuf>,
    /// Rebuild automatically on change (off = react to manual `cargo build` only).
    pub auto_build: bool,
}

impl DevLoopConfig {
    /// Library image path: explicit override or the default under the workspace root.
    pub fn lib_path(&self) -> PathBuf {
        self.lib_path.clone().unwrap_or_else(|| {
            self.workspace_root.join(format!(
                "target/debug/lib{}.so",
                self.package.replace('-', "_")
            ))
        })
    }

    /// Sources watched: explicit override or the package's `src` dir.
    pub fn watch_dir(&self) -> PathBuf {
        self.watch_dir.clone().unwrap_or_else(|| {
            self.workspace_root
                .join("crates")
                .join(&self.package)
                .join("src")
        })
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

/// Watch → build → refresh cycle shared by the dev loops.
pub struct DevLoop {
    config: DevLoopConfig,
    loader: DylibReloader,
    watcher: FileWatcher,
    build_child: Option<Child>,
    /// The constructor already loaded a fresh on-disk library: the next poll reports it
    /// instead of dropping it (otherwise the initial load would never activate).
    pending_initial: bool,
    /// Sources changed while a build was running: rebuild again when it completes.
    ///
    /// Without this, saves landing in the (long) link window are drained from the watcher
    /// and forgotten — the window shows stale UI until the *next* save for no reason.
    dirty_during_build: bool,
}

impl DevLoop {
    /// Creates the loop: watches sources, loads any fresh on-disk library immediately,
    /// and only spawns an initial build when nothing loadable exists (refresh-first:
    /// restarts skip the redundant rebuild and its placeholder flash).
    pub fn new(config: DevLoopConfig) -> Self {
        let lib_path = config.lib_path();
        let watch_dir = config.watch_dir();
        let mut watcher = FileWatcher::new();
        watcher.watch(watch_dir.clone());
        tracing::info!(
            "aurora: hot-reload watching {} (package `{}`)",
            watch_dir.display(),
            config.package
        );
        let mut dev = Self {
            loader: DylibReloader::new(lib_path),
            watcher,
            build_child: None,
            pending_initial: false,
            dirty_during_build: false,
            config,
        };
        if dev.loader.refresh() {
            tracing::info!("aurora: hot-reload library already live; no initial build");
            dev.pending_initial = true;
        } else {
            dev.spawn_build("initial");
        }
        dev
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

    /// Advances the cycle: polls sources and the background build.
    ///
    /// Call once per frame and once per idle wake (an idle app presents no frames, so
    /// frame polling alone would sleep through edits — see the `on_idle` wiring in the
    /// extensions).
    pub fn poll(&mut self) -> PollOutcome {
        if self.pending_initial {
            self.pending_initial = false;
            return PollOutcome::LibraryReady;
        }
        if !self.watcher.poll_changed().is_empty() {
            if self.build_child.is_some() {
                self.dirty_during_build = true;
            } else {
                self.spawn_build("sources changed");
                return PollOutcome::Building;
            }
        }
        if self.build_child.is_some() {
            return self.poll_build();
        }
        // No build running: still pick up libraries changed by hand.
        if self.loader.refresh() {
            return PollOutcome::LibraryReady;
        }
        PollOutcome::Idle
    }

    /// Starts a background `cargo build -p <package>` unless one is already running.
    ///
    /// One command builds the whole closure (shared image first when stale): a single
    /// unit graph means fingerprints agree by construction, which is what keeps
    /// disambiguator skew out. Never add per-build RUSTFLAGS here (see module docs).
    fn spawn_build(&mut self, reason: &str) {
        if !self.config.auto_build || self.build_child.is_some() {
            return;
        }
        match std::process::Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg(&self.config.package)
            .current_dir(&self.config.workspace_root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                tracing::info!(
                    "aurora: hot-reload rebuilding `{}` ({reason})",
                    self.config.package
                );
                self.build_child = Some(child);
            }
            Err(e) => {
                tracing::warn!("aurora: hot-reload could not spawn cargo ({e})");
            }
        }
    }

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
            tracing::warn!(
                "aurora: hot-reload build of `{}` failed; keeping previous UI",
                self.config.package
            );
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
