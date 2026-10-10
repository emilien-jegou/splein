// Single responsibility: Frame-driven real-component code-swap loop as an `AppExtension`.
//
// Runs the shared [`DevLoop`](crate::hot_reload::dev_loop::DevLoop) (watch → background
// build → refresh) and, on every fresh library, resolves the UI crate's component entry
// points and swaps the installed root factory. The engine drain then re-executes the new
// component bodies under the old hook scope: same signals, same springs, tickers
// uninterrupted, with generation-scoped teardown sweeping superseded twins.
//
// Contract with the UI crate (all plain signatures, same toolchain both sides):
// - `aurora_hot_version() -> u64` must equal [`component_abi_fingerprint()`]
//   (layout + toolchain drift guard; the activation also verifies the image bound
//   its aurora imports to this process's live runtime).
// - `aurora_component_root() -> aurora::dsl::Element` rebuilds the interface. It must be
//   callable repeatedly: every reload re-runs it, and hook recall (not fresh state) is
//   what preserves continuity.
//
// Unlike the remote loop, clicks need no action queue: interpreted handlers are real
// closures dispatching through the engine's own input path.

use crate::app::extension::AppExtension;
use crate::dsl::element::Element;
use crate::dsl::{group, text, IntoElement};
use crate::hot_reload::dev_loop::{DevLoop, DevLoopConfig};
use crate::hot_reload::SharedFactory;
use crate::runtime::Engine;
use std::cell::Cell;
use std::rc::Rc;

/// Manual protocol generation, folded into the fingerprint below. Bump on semantic
/// contract changes that layouts alone cannot catch (renamed symbols, changed
/// call order, new required entry points).
const COMPONENT_PROTO_GEN: u64 = 1;

/// Fingerprint both sides must agree on before entry points install.
///
/// Machine-checked, unlike the old hand-bumped version: it folds the protocol
/// generation, the size/alignment of every type crossing the boundary (`Element`),
/// and the compiling toolchain's identity. A signature change, or host and UI built
/// by different toolchains, fails closed with a message instead of misinterpreting
/// memory (or aborting on unwind) across the `extern "Rust"` boundary.
pub fn component_abi_fingerprint() -> u64 {
    let mut h = crate::hot_reload::FNV_OFFSET;
    h = crate::hot_reload::fold_u64(h, COMPONENT_PROTO_GEN);
    h = crate::hot_reload::fold_u64(h, size_of::<Element>() as u64);
    h = crate::hot_reload::fold_u64(h, align_of::<Element>() as u64);
    h = crate::hot_reload::fold_bytes(h, crate::hot_reload::toolchain_id().as_bytes());
    h
}

/// Rebuilds the UI from the currently loaded component library.
pub type ComponentRootFn = extern "Rust" fn() -> Element;
/// Reports the UI crate's ABI fingerprint. See [`component_abi_fingerprint`].
pub type ComponentVersionFn = extern "Rust" fn() -> u64;

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
    /// when the engine should present another frame. Takes its state explicitly so the
    /// shared update lap (`DevLoop::poll_update`) can drive it without borrow fights.
    fn activate_fresh_library(
        dev: &mut DevLoop,
        shared_root: &Rc<Cell<Option<ComponentRootFn>>>,
        factory_installed: &mut bool,
        engine: &mut Engine,
    ) -> bool {
        // Copy the entry points out of the symbols: raw fn pointers carry no lifetime,
        // and the loader (plus its retired images) keeps the code mapped for the session.
        // Single-instance guard first: a fresh image that resolved its aurora imports
        // to a *different* runtime copy (statically-linked driver) would panic its
        // component bodies on an empty scope stack and abort on unwind. Refuse with a
        // message instead; the next save rebuilds and retries.
        match dev.loader().bound_address(b"aurora_runtime_sentinel") {
            Ok(addr) if addr == crate::hot_reload::host_sentinel_address() => {}
            Ok(_) => {
                tracing::error!(
                    "aurora: hot-ui bound aurora to a different runtime instance \
                     (statically-linked driver?); drive via `aurora hot` + libaurora_shared.so; \
                     keeping previous UI"
                );
                return true;
            }
            Err(e) => {
                tracing::warn!("aurora: hot-ui exports no runtime sentinel ({e})");
                return true;
            }
        }
        let root = unsafe {
            let version: libloading::Symbol<ComponentVersionFn> =
                match dev.loader().get(b"aurora_hot_version") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_hot_version` ({e})");
                        return true;
                    }
                };
            if version() != component_abi_fingerprint() {
                tracing::warn!(
                    "aurora: hot-ui ABI fingerprint {:x} != host {:x}; keeping previous UI",
                    version(),
                    component_abi_fingerprint()
                );
                return true;
            }
            let root: libloading::Symbol<ComponentRootFn> =
                match dev.loader().get(b"aurora_component_root") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("aurora: hot-ui missing `aurora_component_root` ({e})");
                        return true;
                    }
                };
            *root
        };
        // The entry address proves a code swap at call level: it must differ from
        // the previously installed one after a rebuild (same bytes relinked keep
        // it — then nothing visible can change and the log says so honestly).
        tracing::info!(
            "aurora: hot-ui entry installed at {:p} (was {:p})",
            root as *const (),
            shared_root.get().map_or(std::ptr::null(), |f| f as *const ()),
        );
        shared_root.set(Some(root));
        if !*factory_installed {
            *factory_installed = true;
            let shared = Rc::clone(shared_root);
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
            dev.loader().retired_count()
        );
        true
    }
}

impl AppExtension for ComponentReloadExtension {
    /// Wakes the loop while the dev cycle needs frames (build running or sources changed).
    fn on_idle(&mut self) -> bool {
        self.dev.poll_idle(&mut self.pending_activation)
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        self.dev.poll_update(
            engine,
            &mut self.pending_activation,
            &mut |dev, engine| {
                Self::activate_fresh_library(
                    dev,
                    &self.shared_root,
                    &mut self.factory_installed,
                    engine,
                )
            },
            &mut || false,
        )
    }
}
