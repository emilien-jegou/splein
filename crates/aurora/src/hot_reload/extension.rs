// Single responsibility: Frame-driven remote-UI dev loop as an `AppExtension`.
//
// Runs the shared [`DevLoop`](crate::hot_reload::dev_loop::DevLoop) (watch → background
// build → refresh) and, on every fresh library, swaps the [`RemoteHost`] entry points and
// queues a state-preserving rebuild through the normal
// [`HotReload`](crate::hot_reload::HotReload) queue. Click actions from the previous frame
// drain first, so interaction never waits for a build.
//
// Typical wiring (see `examples/hot_demo.rs`):
//
// ```ignore
// use aurora::hot_reload::{DevLoopConfig, RemoteReloadExtension};
// use std::path::PathBuf;
//
// let ext = RemoteReloadExtension::new(DevLoopConfig {
//     package: "aurora-hot-ui".to_string(),
//     workspace_root: PathBuf::from("/path/to/splein"),
//     watch_dir: None,
//     lib_path: None,
//     ui_source: None,
//     auto_build: true,
// });
// App::config().extension(ext).run(placeholder);
// ```
//
// The root factory the host installs reads the *current* entry points on every rebuild, so
// a swap followed by `queue_full_reload` is the whole handoff — the engine drain builds
// from the new code under the old hook scope, and every host signal survives.

use crate::app::extension::AppExtension;
use crate::dsl::{group, text, IntoElement};
use crate::hot_reload::dev_loop::{DevLoop, DevLoopConfig, PollOutcome};
use crate::hot_reload::remote::{RemoteBuildFn, RemoteEventFn, RemoteHost, RemoteStateFn};
use crate::hot_reload::SharedFactory;
use crate::runtime::Engine;
use std::cell::Cell;
use std::rc::Rc;

/// Frame-driven remote-UI dev loop. See the module docs.
pub struct RemoteReloadExtension {
    dev: DevLoop,
    host: Rc<RemoteHost>,
    /// Current entry points, read by the installed root factory on every rebuild.
    shared_fns: Rc<Cell<Option<(RemoteStateFn, RemoteBuildFn, RemoteEventFn)>>>,
    factory_installed: bool,
}

impl RemoteReloadExtension {
    /// Creates the loop over `config` (watch + refresh-first initial load).
    pub fn new(config: DevLoopConfig) -> Self {
        Self {
            dev: DevLoop::new(config),
            host: Rc::new(RemoteHost::new()),
            shared_fns: Rc::new(Cell::new(None)),
            factory_installed: false,
        }
    }

    /// The host owning remote state (tests and manual drivers).
    pub fn host(&self) -> Rc<RemoteHost> {
        Rc::clone(&self.host)
    }

    /// Whether the UI library is loaded and entry points are installed.
    pub fn is_live(&self) -> bool {
        self.shared_fns.get().is_some()
    }

    /// Resolves entry points from the fresh library and swaps the factory. Returns true
    /// when the engine should present another frame.
    fn activate_fresh_library(&mut self, engine: &mut Engine) -> bool {
        // Copy the entry points out of the symbols: raw fn pointers carry no lifetime,
        // and the loader (plus its retired images) keeps the code mapped for the session.
        let fns = unsafe {
            let state: libloading::Symbol<RemoteStateFn> =
                match self.dev.loader().get(b"aurora_remote_state") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_remote_state` ({e})");
                        return true;
                    }
                };
            let build: libloading::Symbol<RemoteBuildFn> =
                match self.dev.loader().get(b"aurora_remote_build") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_remote_build` ({e})");
                        return true;
                    }
                };
            let event: libloading::Symbol<RemoteEventFn> =
                match self.dev.loader().get(b"aurora_remote_event") {
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
        tracing::info!(
            "aurora: hot-ui reloaded ({} retired images)",
            self.dev.loader().retired_count()
        );
        true
    }
}

impl AppExtension for RemoteReloadExtension {
    /// Wakes the loop while the dev cycle needs frames (build running or sources changed).
    ///
    /// An idle app presents no frames, so `on_update` alone would never poll the watcher:
    /// this requests redraws until the fresh library is live, then goes quiet again.
    fn on_idle(&mut self) -> bool {
        !matches!(self.dev.poll(), PollOutcome::Idle)
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
        // Dev cycle: watching, building, and hand-built libraries all surface here.
        // `LibraryReady` is the only outcome that swaps entry points.
        match self.dev.poll() {
            PollOutcome::LibraryReady => redraw |= self.activate_fresh_library(engine),
            PollOutcome::Building | PollOutcome::Settled => redraw = true,
            PollOutcome::Idle => {}
        }
        redraw
    }
}
