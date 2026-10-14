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
// - `aurora_hot_window() -> HotWindowInfo` (optional) reports the window chrome
//   the host creates the window with. Missing symbols fall back to defaults,
//   so images predating the window contract stay loadable.
//

/// Window chrome a hot UI crate declares for its window. Plain `repr(C)` data
/// only, so the host can read it straight out of the loaded image.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotWindowInfo {
    /// Logical window width in pixels.
    pub width: u32,
    /// Logical window height in pixels.
    pub height: u32,
    /// Clear color as `0xRRGGBB`.
    pub background: u32,
}

impl HotWindowInfo {
    /// Fallback chrome when the image predates the window contract.
    pub const fn fallback() -> Self {
        Self {
            width: 800,
            height: 600,
            background: 0xFFFFFF,
        }
    }
}
// Unlike the remote loop, clicks need no action queue: interpreted handlers are real
// closures dispatching through the engine's own input path.

use crate::app::extension::{AppExtension, OverlayCaps};
use crate::app::painter::OverlayPainter;
use crate::dsl::element::Element;
use crate::dsl::{group, text, IntoElement};
use crate::foundation::Color;
use crate::hot_reload::dev_loop::{DevLoop, DevLoopConfig};
use crate::hot_reload::progress_bar::ReloadProgressBar;
use crate::hot_reload::SharedFactory;
use crate::runtime::Engine;
use crate::text::TextContext;
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
/// Reports the window chrome the UI crate declares. Optional: images predating
/// the window contract resolve as missing and the host uses [`HotWindowInfo::fallback`].
pub type HotWindowFn = extern "Rust" fn() -> HotWindowInfo;
/// Reports the window title the UI crate declares. Optional, same fallback rule
/// (the host uses a default title when missing).
pub type HotTitleFn = extern "Rust" fn() -> &'static str;

/// Emits the component-loop entry points a hot UI crate must export.
///
/// A hot-reloadable UI is just components plus this contract: the host resolves
/// `aurora_hot_version` (fingerprint handshake) and `aurora_component_root`
/// (rebuilt interface) by exact symbol and signature — a hand-written mismatch is
/// undefined behavior across the boundary, so generate them instead.
/// The block form additionally emits `aurora_hot_window` / `aurora_hot_title`,
/// which the host reads to size and title the window *before* creating it:
///
/// ```ignore
/// extern crate aurora_shared as aurora;
///
/// #[component(name = "shop::Page")]
/// fn Page() -> impl IntoElement { /* ... */ }
///
/// aurora::define_hot_ui! {
///     root: view::Page,
///     title: view::HOT_TITLE,
///     size: (view::HOT_WIDTH, view::HOT_HEIGHT),
///     background: view::HOT_BG,
/// }
/// ```
///
/// `::aurora` paths resolve both through the shared-instance alias (UI crates) and
/// through a direct `aurora` dependency (ordinary crates), so one expansion serves
/// both sides.
#[macro_export]
macro_rules! define_hot_ui {
    ($root:path) => {
        /// ABI fingerprint the host checks before installing entry points.
        #[no_mangle]
        pub extern "Rust" fn aurora_hot_version() -> u64 {
            ::aurora::hot_reload::component_abi_fingerprint()
        }

        /// Rebuilds the interface. Re-run on every reload under the old hook scope.
        #[no_mangle]
        pub extern "Rust" fn aurora_component_root() -> ::aurora::dsl::Element {
            use ::aurora::dsl::IntoElement;
            $root().into_element()
        }
    };
    {
        root: $root:path,
        title: $title:expr,
        size: ($width:expr, $height:expr),
        background: $background:expr $(,)?
    } => {
        $crate::define_hot_ui!($root);

        /// Window chrome the host creates the window with. Read once before
        /// the window exists, re-read on every reload for live resize.
        #[no_mangle]
        pub extern "Rust" fn aurora_hot_window() -> ::aurora::hot_reload::HotWindowInfo {
            ::aurora::hot_reload::HotWindowInfo {
                width: $width,
                height: $height,
                background: $background,
            }
        }

        /// Window title the host applies. A plain `&'static str`: same
        /// toolchain both sides, and the boundary already ships richer types.
        #[no_mangle]
        pub extern "Rust" fn aurora_hot_title() -> &'static str {
            $title
        }
    };
}
/// Frame-driven real-component code-swap loop. See the module docs.
pub struct ComponentReloadExtension {
    dev: DevLoop,
    /// Current root entry point, read by the installed factory on every rebuild.
    shared_root: Rc<Cell<Option<ComponentRootFn>>>,
    factory_installed: bool,
    /// A fresh library was observed where no engine was available (idle poll). Latched
    /// so the next frame swaps instead of the observation being lost.
    pending_activation: bool,
    /// Window chrome resolved from the fresh library, awaiting application in
    /// `on_window` (the only place with a window handle). `None` when the
    /// image predates the window contract or nothing changed.
    pending_chrome: Option<(HotWindowInfo, String)>,
    /// Last applied chrome: re-applied only on change, so const edits
    /// live-resize without redundant requests. `None` until the first dress.
    applied_chrome: Option<(HotWindowInfo, String)>,
    /// Boot freeze: the clock stays at 0 until the first UI library installs,
    /// so drift demos reveal at t≈0 instead of mid-travel. Cleared once —
    /// unless the user pressed F8 during the build (tracked via `on_event`),
    /// in which case their freeze stands and no override happens.
    boot_freeze_armed: bool,
    user_froze: bool,
    /// Top loading bar shown while the loop is rebuilding.
    bar: ReloadProgressBar,
}

impl ComponentReloadExtension {
    /// Creates the loop over `config` (watch + refresh-first initial load).
    pub fn new(config: DevLoopConfig) -> Self {
        Self {
            dev: DevLoop::new(config),
            shared_root: Rc::new(Cell::new(None)),
            factory_installed: false,
            pending_activation: false,
            pending_chrome: None,
            applied_chrome: None,
            boot_freeze_armed: true,
            user_froze: false,
            bar: ReloadProgressBar::new(),
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
        pending_chrome: &mut Option<(HotWindowInfo, String)>,
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
        // Window chrome rides the same image: optional symbols, declared by
        // the block form of `define_hot_ui!`. Missing (predating images) falls
        // back to defaults, silently — the handshake already guards real drift.
        let chrome: Option<(HotWindowInfo, String)> = unsafe {
            let info = match dev.loader().get::<HotWindowFn>(b"aurora_hot_window") {
                Ok(f) => f(),
                Err(_) => HotWindowInfo::fallback(),
            };
            let title = match dev.loader().get::<HotTitleFn>(b"aurora_hot_title") {
                Ok(f) => f().to_string(),
                Err(_) => "Aurora Hot Reload".to_string(),
            };
            Some((info, title))
        };
        *pending_chrome = chrome;
        // The entry address proves a code swap at call level: it must differ from
        // the previously installed one after a rebuild (same bytes relinked keep
        // it — then nothing visible can change and the log says so honestly).
        tracing::info!(
            "aurora: hot-ui entry installed at {:p} (was {:p})",
            root as *const (),
            shared_root
                .get()
                .map_or(std::ptr::null(), |f| f as *const ()),
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

    /// Tracks an F8 press during the initial build so the first-load unfreeze
    /// below never overrides a freeze the user asked for explicitly.
    #[cfg(feature = "desktop")]
    fn on_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        if let winit::event::WindowEvent::KeyboardInput { event: key, .. } = event {
            if key.state == winit::event::ElementState::Pressed
                && key.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::F8)
            {
                self.user_froze = true;
            }
        }
        false
    }

    /// Applies pending window chrome (resolved at activation) and owns the
    /// boot freeze. Runs only on change after the first dress, so const edits
    /// live-resize without redundant requests.
    #[cfg(feature = "desktop")]
    fn on_window(&mut self, window: &winit::window::Window, engine: &mut Engine) {
        let Some((info, title)) = self.pending_chrome.take() else {
            return;
        };
        if self.applied_chrome.as_ref() != Some(&(info, title.clone()))
            || self.boot_freeze_armed
        {
            engine.set_background(Color::hex(info.background));
            window.set_title(&title);
            let _ = window
                .request_inner_size(winit::dpi::LogicalSize::new(info.width.max(1), info.height.max(1)));
            window.set_visible(true);
            self.applied_chrome = Some((info, title));
        }
        if self.boot_freeze_armed {
            self.boot_freeze_armed = false;
            if !self.user_froze {
                engine.set_time_scale(1.0);
            }
        }
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        // Boot freeze: hold the clock at 0 until the first UI library
        // installs, so drift demos reveal at t≈0. Idempotent; cleared by
        // `on_window` on first dress (or left for the user when F8 intervened).
        if self.boot_freeze_armed && !self.is_live() {
            engine.set_time_scale(0.0);
        }
        let redraw = self.dev.poll_update(
            engine,
            &mut self.pending_activation,
            &mut |dev, engine| {
                Self::activate_fresh_library(
                    dev,
                    &self.shared_root,
                    &mut self.pending_chrome,
                    &mut self.factory_installed,
                    engine,
                )
            },
            &mut || false,
        );
        // Tier-1 template fast path: live values staged by the dev loop apply
        // against the *existing* factory — no cargo, no dlopen. A zero-apply
        // (shapes shifted under a textually template-only edit) falls back to
        // a real rebuild rather than showing stale UI.
        let mut fast_redraw = false;
        if let Some(pairs) = self.dev.take_pending_fast() {
            let n = crate::hot_reload::template::apply_template_delta(&pairs, &[]);
            if n > 0 {
                engine.queue_full_reload();
                fast_redraw = true;
                tracing::info!(
                    "aurora: hot-ui template values applied ({n} slots, no rebuild)"
                );
            } else {
                self.dev.request_build("template fast-path miss");
            }
        }
        // The loop state was just published to stats; advance the loading bar
        // from it (simulated time, so freeze/slow-motion apply to the bar too).
        // NOTE: update the bar unconditionally — `||` would short-circuit and
        // never run it on exactly the frames that matter (`redraw` is true for
        // the whole build), leaving the bar stuck Hidden forever.
        let stats = engine.hot_reload_stats();
        let sim = engine.diagnostics().simulated;
        let w = engine.logical_size().0 as f32;
        let bar_needs_frames = self.bar.update(stats.dev_state, sim, w);
        redraw || bar_needs_frames || fast_redraw
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
