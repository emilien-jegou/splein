// Single responsibility: Frame-driven real-component code-swap loop as an `AppExtension`.
//
// Runs the shared [`DevLoop`](crate::hot_reload::dev_loop::DevLoop) (watch → background
// build → refresh) and, on every fresh library, resolves the UI crate's component entry
// points and swaps the installed root factory. The engine drain then re-executes the new
// component bodies under the old hook scope: same signals, same springs, tickers
// uninterrupted, with generation-scoped teardown sweeping superseded twins.
//
// Contract with the UI crate (all plain signatures, same toolchain both sides):
// - `aurora_hot_version() -> u32` must equal [`COMPONENT_ABI_VERSION`] (protocol drift
//   guard; toolchain skew is still the author's responsibility).
// - `aurora_component_root() -> aurora::dsl::Element` rebuilds the interface. It must be
//   callable repeatedly: every reload re-runs it, and hook recall (not fresh state) is
//   what preserves continuity.
//
// Unlike the remote loop, clicks need no action queue: interpreted handlers are real
// closures dispatching through the engine's own input path.

use crate::app::extension::AppExtension;
use crate::dsl::element::Element;
use crate::dsl::{group, text, IntoElement};
use crate::hot_reload::dev_loop::{DevLoop, DevLoopConfig, PollOutcome};
use crate::hot_reload::SharedFactory;
use crate::runtime::Engine;
use std::cell::Cell;
use std::rc::Rc;

/// Component-protocol version both sides agree on. Bump on any signature change.
pub const COMPONENT_ABI_VERSION: u32 = 1;

/// Rebuilds the UI from the currently loaded component library.
pub type ComponentRootFn = extern "Rust" fn() -> Element;
/// Reports the UI crate's protocol version. See [`COMPONENT_ABI_VERSION`].
pub type ComponentVersionFn = extern "Rust" fn() -> u32;

/// Frame-driven real-component code-swap loop. See the module docs.
pub struct ComponentReloadExtension {
    dev: DevLoop,
    /// Current root entry point, read by the installed factory on every rebuild.
    shared_root: Rc<Cell<Option<ComponentRootFn>>>,
    factory_installed: bool,
    /// A fresh library was observed where no engine was available (idle poll). Latched
    /// so the next frame swaps instead of the observation being lost.
    pending_activation: bool,
}

impl ComponentReloadExtension {
    /// Creates the loop over `config` (watch + refresh-first initial load).
    pub fn new(config: DevLoopConfig) -> Self {
        Self {
            dev: DevLoop::new(config),
            shared_root: Rc::new(Cell::new(None)),
            factory_installed: false,
            pending_activation: false,
        }
    }

    /// Whether a component library is loaded and installed.
    pub fn is_live(&self) -> bool {
        self.shared_root.get().is_some()
    }

    /// Resolves entry points from the fresh library and swaps the factory. Returns true
    /// when the engine should present another frame.
    fn activate_fresh_library(&mut self, engine: &mut Engine) -> bool {
        // Copy the entry points out of the symbols: raw fn pointers carry no lifetime,
        // and the loader (plus its retired images) keeps the code mapped for the session.
        let root = unsafe {
            let version: libloading::Symbol<ComponentVersionFn> =
                match self.dev.loader().get(b"aurora_hot_version") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_hot_version` ({e})");
                        return true;
                    }
                };
            if version() != COMPONENT_ABI_VERSION {
                tracing::warn!(
                    "aurora: hot-ui protocol version {} != host {}; keeping previous UI",
                    version(),
                    COMPONENT_ABI_VERSION
                );
                return true;
            }
            let root: libloading::Symbol<ComponentRootFn> =
                match self.dev.loader().get(b"aurora_component_root") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_component_root` ({e})");
                        return true;
                    }
                };
            *root
        };
        self.shared_root.set(Some(root));
        if !self.factory_installed {
            self.factory_installed = true;
            let shared = Rc::clone(&self.shared_root);
            // Re-runs the *current* entry point on every rebuild: after a swap this is new
            // code under the old hook scope (state preserved); before any swap it cannot
            // happen (the factory installs together with the first entry points).
            let factory: SharedFactory = Rc::new(move || {
                let Some(root) = shared.get() else {
                    return group()
                        .children([text("building hot-ui…").size(16.0)])
                        .into_element();
                };
                root()
            });
            engine.set_root_factory(factory);
        }
        engine.queue_full_reload();
        tracing::info!(
            "aurora: hot-ui components reloaded ({} retired images)",
            self.dev.loader().retired_count()
        );
        true
    }
}

impl AppExtension for ComponentReloadExtension {
    /// Wakes the loop while the dev cycle needs frames (build running or sources changed).
    /// A `LibraryReady` observed here latches `pending_activation` (no engine access to
    /// swap with) so the next frame swaps instead of dropping the observation.
    fn on_idle(&mut self) -> bool {
        if self.pending_activation {
            return true;
        }
        match self.dev.poll() {
            PollOutcome::LibraryReady => {
                self.pending_activation = true;
                true
            }
            PollOutcome::Building | PollOutcome::Settled => true,
            PollOutcome::Idle => false,
        }
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        let mut redraw = false;
        if self.pending_activation {
            self.pending_activation = false;
            redraw |= self.activate_fresh_library(engine);
        }
        match self.dev.poll() {
            PollOutcome::LibraryReady => redraw |= self.activate_fresh_library(engine),
            PollOutcome::Building | PollOutcome::Settled => redraw = true,
            PollOutcome::Idle => {}
        }
        // A failed build explains itself through stats (HUD/panel read these); the loop
        // itself waits for the next save rather than retrying blindly.
        if let Some((cause, log)) = self.dev.take_build_error() {
            engine.hot_reload_mut().record_build_failure(cause, log);
            redraw = true;
        }
        redraw
    }
}
