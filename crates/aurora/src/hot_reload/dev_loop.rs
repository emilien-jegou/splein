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
// appends exactly one build flag of its own — `--sysroot` pinned to the toolchain's
// reported sysroot — because split toolchain installations expose every sysroot crate
// twice and dylib-consuming links fail with "only shows up once" otherwise. Keep hand
// builds vanilla otherwise: exotic per-build flags multiply unit hashes and produce the
// mixed-invocation skew this loop is designed never to see.

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

    /// Package manifest watched alongside sources: dependency and feature edits rebuild
    /// through the same cycle as source edits. Missing manifests are simply unwatched.
    pub fn manifest_path(&self) -> PathBuf {
        self.workspace_root
            .join("crates")
            .join(&self.package)
            .join("Cargo.toml")
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
    /// Cargo log of the running/finished build (stderr). Overwritten per spawn; read as
    /// a tail on failure so a broken edit explains itself through stats instead of
    /// requiring a terminal.
    build_log: PathBuf,
    /// Last failed build waiting to be reported: one-line cause plus stderr tail.
    /// Set on failure, consumed once by `take_build_error`.
    build_error: Option<(String, Vec<String>)>,
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
        // Manifest edits (deps, features) rebuild like source edits; a missing manifest
        // is inert (watch logs and ignores unwatched paths).
        watcher.watch(config.manifest_path());
        tracing::info!(
            "aurora: hot-reload watching {} (package `{}`)",
            watch_dir.display(),
            config.package
        );
        let build_log = std::env::temp_dir()
            .join(format!("aurora-hot-build-{}.log", std::process::id()));
        let mut dev = Self {
            loader: DylibReloader::new(lib_path),
            watcher,
            build_child: None,
            build_log,
            build_error: None,
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

    /// Takes the last failed build's summary, if one is waiting.
    ///
    /// Consuming: each failure reports exactly once, to exactly one poller. Returns the
    /// one-line cause plus the captured stderr tail (newest last, capped).
    pub fn take_build_error(&mut self) -> Option<(String, Vec<String>)> {
        self.build_error.take()
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
    /// disambiguator skew out. The build pins `--sysroot` to the toolchain's own
    /// reported sysroot (see `rustc_sysroot`): split toolchain installations expose the
    /// same sysroot crates twice, and dylib-consuming links fail with "only shows up
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
        let mut command = std::process::Command::new("cargo");
        command
            .arg("build")
            .arg("-p")
            .arg(&self.config.package)
            .current_dir(&self.config.workspace_root)
            .stdout(std::process::Stdio::null())
            .stderr(log_file);
        if let Some(sysroot) = rustc_sysroot() {
            let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
            if !rustflags.is_empty() {
                rustflags.push(' ');
            }
            rustflags.push_str("--sysroot ");
            rustflags.push_str(&sysroot.to_string_lossy());
            command.env("RUSTFLAGS", rustflags);
        } else {
            tracing::warn!(
                "aurora: hot-reload could not determine rustc sysroot; dylib links may fail"
            );
        }
        match command.spawn()
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
            let log = read_tail(&self.build_log, 20);
            let cause = format!("cargo build -p {} failed", self.config.package);
            tracing::warn!("aurora: hot-reload {cause}; keeping previous UI");
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
            package: "aurora-hot-nonexistent-xyz".to_string(),
            workspace_root: root,
            watch_dir: None,
            lib_path: None,
            auto_build: true,
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
            package: "pkg".to_string(),
            workspace_root: root.clone(),
            watch_dir: None,
            lib_path: None,
            auto_build: false,
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

/// Reports the toolchain's own sysroot (`rustc --print sysroot`), if rustc runs.
///
/// Used to pin `--sysroot` on loop-spawned builds (see `spawn_build`). Resolved from
/// `$RUSTC` when set, else `PATH` — the same rustc cargo would use. `None` only when
/// rustc itself cannot run, in which case the build about to spawn would fail anyway.
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
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

#[cfg(test)]
mod sysroot_tests {
    #[test]
    fn sysroot_discovery_points_at_a_sysroot() {
        let Some(sysroot) = super::rustc_sysroot() else {
            return;
        };
        assert!(
            sysroot.join("lib/rustlib").exists() || sysroot.join("lib").exists(),
            "discovered sysroot looks like a sysroot: {}",
            sysroot.display()
        );
    }
}
