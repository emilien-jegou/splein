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
// use riode::hot_reload::{DevLoopConfig, RemoteReloadExtension};
// use std::path::PathBuf;
//
// let ext = RemoteReloadExtension::new(DevLoopConfig {
//     package: "your-package".to_string(),
//     workspace_root: PathBuf::from("/path/to/splein"),
//     watch_dir: None,
//     lib_path: None,
//     auto_build: true,
// });
// App::config().extension(ext).run(placeholder);
// ```
//
// The root factory the host installs reads the *current* entry points on every rebuild, so
// a swap followed by `queue_full_reload` is the whole handoff — the engine drain builds
// from the new code under the old hook scope, and every host signal survives.

use crate::app::extension::{AppExtension, OverlayCaps};
use crate::app::painter::OverlayPainter;
use crate::dsl::{group, text, IntoElement};
use crate::hot_reload::dev_loop::{DevLoop, DevLoopConfig};
use crate::hot_reload::progress_bar::ReloadProgressBar;
use crate::hot_reload::remote::{
    remote_abi_fingerprint, RemoteBuildFn, RemoteEventFn, RemoteHost, RemoteStateFn,
    RemoteVersionFn,
};
use crate::hot_reload::SharedFactory;
use crate::runtime::Engine;
use crate::text::TextContext;
use std::cell::Cell;
use std::rc::Rc;

/// Frame-driven remote-UI dev loop. See the module docs.
pub struct RemoteReloadExtension {
    dev: DevLoop,
    host: Rc<RemoteHost>,
    /// Current entry points, read by the installed root factory on every rebuild.
    shared_fns: Rc<Cell<Option<(RemoteStateFn, RemoteBuildFn, RemoteEventFn)>>>,
    factory_installed: bool,
    /// A fresh library was observed where no engine was available (idle poll). Latched
    /// so the next frame swaps instead of the observation being lost.
    pending_activation: bool,
    /// Top loading bar shown while the loop is rebuilding.
    bar: ReloadProgressBar,
}

impl RemoteReloadExtension {
    /// Creates the loop over `config` (watch + refresh-first initial load).
    pub fn new(config: DevLoopConfig) -> Self {
        Self {
            dev: DevLoop::new(config),
            host: Rc::new(RemoteHost::new()),
            shared_fns: Rc::new(Cell::new(None)),
            factory_installed: false,
            pending_activation: false,
            bar: ReloadProgressBar::new(),
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
    /// when the engine should present another frame. Takes its state explicitly so the
    /// shared update lap (`DevLoop::poll_update`) can drive it without borrow fights.
    fn activate_fresh_library(
        dev: &mut DevLoop,
        host: &Rc<RemoteHost>,
        shared_fns: &Rc<Cell<Option<(RemoteStateFn, RemoteBuildFn, RemoteEventFn)>>>,
        factory_installed: &mut bool,
        engine: &mut Engine,
    ) -> bool {
        // Single-instance guard first, like the component loop: an image bound to a
        // different runtime copy would misbehave across the boundary. Refuse with a
        // message instead; the next save rebuilds and retries.
        match dev.loader().bound_address(b"riode_runtime_sentinel") {
            Ok(addr) if addr == crate::hot_reload::host_sentinel_address() => {}
            Ok(_) => {
                tracing::error!(
                    "riode: hot-ui bound riode to a different runtime instance \
                     (statically-linked driver?); drive via `riode hot` + libriode_shared.so; \
                     keeping previous UI"
                );
                return true;
            }
            Err(e) => {
                tracing::warn!("riode: hot-ui exports no runtime sentinel ({e})");
                return true;
            }
        }
        // Copy the entry points out of the symbols: raw fn pointers carry no lifetime,
        // and the loader (plus its retired images) keeps the code mapped for the session.
        // The fingerprint check comes first so protocol drift fails closed with a
        // message instead of misinterpreting a foreign description.
        let fns = unsafe {
            let version: libloading::Symbol<RemoteVersionFn> =
                match dev.loader().get(b"riode_remote_version") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("riode: hot-ui missing `riode_remote_version` ({e})");
                        return true;
                    }
                };
            if version() != remote_abi_fingerprint() {
                tracing::warn!(
                    "riode: hot-ui remote ABI fingerprint {:x} != host {:x}; keeping previous UI",
                    version(),
                    remote_abi_fingerprint()
                );
                return true;
            }
            let state: libloading::Symbol<RemoteStateFn> =
                match dev.loader().get(b"riode_remote_state") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("riode: hot-ui missing `riode_remote_state` ({e})");
                        return true;
                    }
                };
            let build: libloading::Symbol<RemoteBuildFn> =
                match dev.loader().get(b"riode_remote_build") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("riode: hot-ui missing `riode_remote_build` ({e})");
                        return true;
                    }
                };
            let event: libloading::Symbol<RemoteEventFn> =
                match dev.loader().get(b"riode_remote_event") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("riode: hot-ui missing `riode_remote_event` ({e})");
                        return true;
                    }
                };
            (*state, *build, *event)
        };
        // The host is shared with the installed root factory (both hold an `Rc`), and it
        // is interior-mutable throughout — attach, declare, and swap freely, even mid-frame.
        host.attach_runtime(engine.runtime());
        host.set_fns(fns.0, fns.1, fns.2);
        if !host.ensure_state(&(fns.0)()) {
            tracing::warn!("riode: hot-ui has no runtime yet; retrying next frame");
            return true;
        }
        shared_fns.set(Some(fns));
        if !*factory_installed {
            *factory_installed = true;
            let host = Rc::clone(host);
            let shared = Rc::clone(shared_fns);
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
            "riode: hot-ui reloaded ({} retired images)",
            dev.loader().retired_count()
        );
        true
    }
}

impl AppExtension for RemoteReloadExtension {
    /// Wakes the loop while the dev cycle needs frames (build running or sources changed).
    ///
    /// An idle app presents no frames, so `on_update` alone would never poll the watcher:
    /// this requests redraws until the fresh library is live, then goes quiet again.
    /// A `LibraryReady` observed here cannot activate (no engine access) — it latches
    /// `pending_activation` so the next `on_update` swaps instead of dropping it.
    fn on_idle(&mut self) -> bool {
        self.dev.poll_idle(&mut self.pending_activation)
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        let redraw = self.dev.poll_update(
            engine,
            &mut self.pending_activation,
            &mut |dev, engine| {
                Self::activate_fresh_library(
                    dev,
                    &self.host,
                    &self.shared_fns,
                    &mut self.factory_installed,
                    engine,
                )
            },
            &mut || {
                // Interaction first: clicks from the last presented frame become state
                // writes now, so the UI answers in one frame whether or not a build
                // is in flight.
                let actions = self.host.drain_actions();
                if actions.is_empty() {
                    return false;
                }
                if let Some((_, _, event)) = self.shared_fns.get() {
                    let mut writes = Vec::new();
                    for action in &actions {
                        writes.extend(event(action));
                    }
                    self.host.apply_writes(&writes);
                }
                true
            },
        );
        let stats = engine.hot_reload_stats();
        let sim = engine.diagnostics().simulated;
        let w = engine.logical_size().0 as f32;
        // NOTE: update the bar unconditionally — `||` would short-circuit and
        // never run it on exactly the frames that matter (`redraw` is true for
        // the whole build), leaving the bar stuck Hidden forever.
        let bar_needs_frames = self.bar.update(stats.dev_state, sim, w);
        redraw || bar_needs_frames
    }

    fn render_overlay(
        &mut self,
        paint: &mut OverlayPainter<'_>,
        _text_ctx: &TextContext,
        caps: &OverlayCaps,
        _d: &crate::runtime::FrameDiagnostics,
    ) {
        self.bar.draw(paint, caps);
    }

    fn overlay_damage(&self) -> Option<crate::foundation::ResolvedRect> {
        self.bar.damage()
    }
}
