// Single responsibility: Frame-driven remote-UI dev loop as an `AppExtension`.
//
// Watches the UI crate's sources, rebuilds it in the background on change, loads the fresh
// `cdylib` through [`DylibReloader`], swaps the [`RemoteHost`] entry points, and queues a
// state-preserving rebuild through the normal [`HotReload`](crate::hot_reload::HotReload)
// queue. Click actions from the previous frame drain first, so interaction never waits for
// a build.
//
// Typical wiring (see `examples/hot_demo.rs`):
//
// ```ignore
// let ext = RemoteReloadExtension::new(RemoteReloadConfig {
//     package: "aurora-hot-ui".to_string(),
//     workspace_root: workspace_root(),
//     ..RemoteReloadConfig::default()
// });
// App::config().extension(ext).run(placeholder);
// ```
//
// The root factory the host installs reads the *current* entry points on every rebuild, so
// a swap followed by `queue_full_reload` is the whole handoff — the engine drain builds
// from the new code under the old hook scope, and every host signal survives.

use crate::app::extension::AppExtension;
use crate::dsl::{group, text, IntoElement};
use crate::hot_reload::dylib::DylibReloader;
use crate::hot_reload::remote::{RemoteBuildFn, RemoteEventFn, RemoteHost, RemoteStateFn};
use crate::hot_reload::watcher::FileWatcher;
use crate::hot_reload::SharedFactory;
use crate::runtime::Engine;
use std::cell::Cell;
use std::path::PathBuf;
use std::process::Child;
use std::rc::Rc;

/// What to build, watch, and load for remote UI hot-reload.
pub struct RemoteReloadConfig {
    /// Cargo package whose `cdylib` holds the UI (`cargo build -p …` rebuilds it).
    pub package: String,
    /// Workspace root: build working directory and anchor for default paths.
    pub workspace_root: PathBuf,
    /// Sources watched for changes. Defaults to `crates/<package>/src`.
    pub watch_dir: Option<PathBuf>,
    /// Library image loaded. Defaults to `target/debug/lib<package>.so`.
    pub lib_path: Option<PathBuf>,
    /// Rebuild automatically on change (off = react to manual `cargo build` only).
    pub auto_build: bool,
}

impl RemoteReloadConfig {
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

/// Frame-driven remote-UI dev loop. See the module docs.
pub struct RemoteReloadExtension {
    config: RemoteReloadConfig,
    loader: DylibReloader,
    watcher: FileWatcher,
    host: Rc<RemoteHost>,
    /// Current entry points, read by the installed root factory on every rebuild.
    shared_fns: Rc<Cell<Option<(RemoteStateFn, RemoteBuildFn, RemoteEventFn)>>>,
    build_child: Option<Child>,
    factory_installed: bool,
}

impl RemoteReloadExtension {
    /// Creates the loop: watches sources and kicks off the initial build immediately.
    pub fn new(config: RemoteReloadConfig) -> Self {
        let lib_path = config.lib_path();
        let watch_dir = config.watch_dir();
        let mut watcher = FileWatcher::new();
        watcher.watch(watch_dir.clone());
        tracing::info!(
            "aurora: hot-reload watching {} (package `{}`)",
            watch_dir.display(),
            config.package
        );
        let mut ext = Self {
            loader: DylibReloader::new(lib_path),
            watcher,
            host: Rc::new(RemoteHost::new()),
            shared_fns: Rc::new(Cell::new(None)),
            build_child: None,
            factory_installed: false,
            config,
        };
        ext.spawn_build("initial");
        ext
    }

    /// The host owning remote state (tests and manual drivers).
    pub fn host(&self) -> Rc<RemoteHost> {
        Rc::clone(&self.host)
    }

    /// Whether the UI library is loaded and entry points are installed.
    pub fn is_live(&self) -> bool {
        self.shared_fns.get().is_some()
    }

    /// Starts a background `cargo build -p <package>` unless one is already running.
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
                tracing::info!("aurora: hot-reload rebuilding `{}` ({reason})", self.config.package);
                self.build_child = Some(child);
            }
            Err(e) => {
                tracing::warn!("aurora: hot-reload could not spawn cargo ({e})");
            }
        }
    }

    /// Polls the background build; on success loads the fresh library and swaps entry points.
    fn poll_build(&mut self, engine: &mut Engine) -> bool {
        let done = match self.build_child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => Some(status.success()),
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!("aurora: hot-reload build poll failed ({e})");
                    Some(false)
                }
            },
            None => None,
        };
        let Some(success) = done else {
            return false;
        };
        self.build_child = None;
        if !success {
            tracing::warn!(
                "aurora: hot-reload build of `{}` failed; keeping previous UI",
                self.config.package
            );
            return true;
        }
        self.activate_fresh_library(engine)
    }

    /// Loads the library at its path (if changed) and swaps entry points + factory.
    fn activate_fresh_library(&mut self, engine: &mut Engine) -> bool {
        if !self.loader.refresh() {
            return false;
        }
        // Copy the entry points out of the symbols: raw fn pointers carry no lifetime,
        // and the loader (plus its retired images) keeps the code mapped for the session.
        let fns = unsafe {
            let state: libloading::Symbol<RemoteStateFn> =
                match self.loader.get(b"aurora_remote_state") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_remote_state` ({e})");
                        return true;
                    }
                };
            let build: libloading::Symbol<RemoteBuildFn> =
                match self.loader.get(b"aurora_remote_build") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_remote_build` ({e})");
                        return true;
                    }
                };
            let event: libloading::Symbol<RemoteEventFn> =
                match self.loader.get(b"aurora_remote_event") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_remote_event` ({e})");
                        return true;
                    }
                };
            (*state, *build, *event)
        };
        // The host is shared with the installed root factory (both hold an `Rc`), and it
        // is interior-mutable throughout — attach, declare, and swap freely, even mid-frame.
        self.host.attach_runtime(engine.runtime());
        self.host.set_fns(fns.0, fns.1, fns.2);
        if !self.host.ensure_state(&(fns.0)()) {
            tracing::warn!("aurora: hot-ui has no runtime yet; retrying next frame");
            return true;
        }
        self.shared_fns.set(Some(fns));
        if !self.factory_installed {
            self.factory_installed = true;
            let host = Rc::clone(&self.host);
            let shared = Rc::clone(&self.shared_fns);
            let factory: SharedFactory = Rc::new(move || {
                let Some((_, build, _)) = shared.get() else {
                    return group()
                        .children([text("building hot-ui…").size(16.0)])
                        .into_element();
                };
                // Snapshot-then-interpret: the dylib branches on current state, the host
                // owns every signal the description binds.
                let node = build(&host.snapshot());
                host.interpret(&node)
            });
            engine.set_root_factory(factory);
        }
        engine.queue_full_reload();
        tracing::info!("aurora: hot-ui reloaded ({} retired images)", self.loader.retired_count());
        true
    }
}

impl AppExtension for RemoteReloadExtension {
    /// Wakes the loop while a build runs or sources changed.
    ///
    /// An idle app presents no frames, so `on_update` alone would never poll the watcher:
    /// this requests redraws until the fresh library is live, then goes quiet again.
    fn on_idle(&mut self) -> bool {
        if self.build_child.is_some() {
            return true;
        }
        if !self.watcher.poll_changed().is_empty() {
            self.spawn_build("sources changed");
            return true;
        }
        false
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        // Interaction first: clicks from the last presented frame become state writes now,
        // so the UI answers in one frame whether or not a build is in flight.
        let actions = self.host.drain_actions();
        let mut redraw = !actions.is_empty();
        if !actions.is_empty() {
            if let Some((_, _, event)) = self.shared_fns.get() {
                let mut writes = Vec::new();
                for action in &actions {
                    writes.extend(event(action));
                }
                self.host.apply_writes(&writes);
            }
        }
        // New sources → rebuild in the background.
        if !self.watcher.poll_changed().is_empty() {
            self.spawn_build("sources changed");
            redraw = true;
        }
        // Build finished → swap. Library changed by hand → pick it up too.
        if self.poll_build(engine) {
            redraw = true;
        } else if self.build_child.is_none() {
            redraw |= self.activate_fresh_library(engine);
        }
        redraw
    }
}
