// Single responsibility: Encapsulated UI engine orchestrating layout, scene compilation, and invalidations.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::dsl::element::Element;
use crate::dsl::reconcile_group::replace_child;
use crate::dsl::reconciler::reconcile;
use crate::dsl::IntoElement;
use crate::foundation::{Color, DamageRegion, DamageRing, Key, Point, ResolvedRect, Transform};
use crate::hot_reload::{HotReload, HotReloadStats, ReloadReport, SharedFactory};
use crate::input::{CursorIcon, KeyEvent, PointerEvent, PointerOutcome, WheelInfo};
use crate::reactive::{Cx, ReactiveRuntime, Signal};
use crate::runtime::diagnostics::EngineDiagnostics;
use crate::runtime::frame::run_frame_passes;
use crate::runtime::node_query::NodeQuery;
use crate::runtime::pointer_input::PointerInput;
use crate::runtime::scheduler::FrameScheduler;
use crate::runtime::FrameReport;
use crate::scene::{LayerId, Scene};
use crate::text::{TextContext, TextSelection};
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId, SubscriberRouter};

/// Single-point facade managing the UI tree, reactive runtime, and scene compilation.
pub struct Engine {
    scheduler: FrameScheduler,
    scene: Scene,
    logical_size: (u32, u32),
    background: Color,
    damage_ring: DamageRing,
    pending_damage: DamageRegion,
    frame_counter: u64,
    pending_preserved_canvas_rect: Option<ResolvedRect>,
    /// Pointer routing: hit resolution, press capture, gesture recognition, and the queue
    /// of events waiting for the next frame.
    pointer: PointerInput,
    /// Key presses waiting for the next frame, drained after pointer events so a
    /// press that moves focus lands before the key that uses it.
    queued_keys: Vec<KeyEvent>,
    /// Wheel scrolls waiting for the next frame, drained with the keys.
    queued_wheels: Vec<WheelInfo>,
    /// Last published caret blink phase, so only flips repaint.
    caret_blink_on: bool,
    /// Focused caret seen by the last blink poll, to restart blink on change.
    caret_last_seen: Option<(NodeId, TextSelection)>,
    /// Animation time the caret last changed; the blink phase counts from here.
    caret_epoch: f32,
    /// Panic message from the last frame that unwound, if any. Set when the
    /// frame boundary catches a panic (binding eval, ticker, paint); cleared
    /// on the next clean frame. Surfaces in the HUD and survives for
    /// hot-reload loops to display. Surviving a frame is the point: a bad
    /// literal or a buggy handler drops one frame, never the app.
    last_frame_error: Option<String>,
    /// Hot-reload orchestrator: queued rebuilds, component registry, file watcher.
    ///
    /// Dormant until used — with no factory and nothing queued, the per-frame drain is a
    /// flag check. See [`crate::hot_reload`].
    hot_reload: HotReload,
    /// Hot-reload failure staged for the end of the frame.
    ///
    /// A failed rebuild must stay visible in [`Self::last_frame_error`] for a full frame:
    /// recording it in the drain and letting the pass below clear it would hide it inside
    /// the same `frame()` call. Applied after the pass outcome so it wins over the clean
    /// slate, then clears on the next clean frame like any other frame error.
    staged_reload_error: Option<String>,
}

impl Engine {
    /// Creates an engine instance configured with an existing reactive runtime.
    pub fn with_runtime(
        runtime: Rc<RefCell<ReactiveRuntime>>,
        root: Element,
        width: u32,
        height: u32,
    ) -> Self {
        // The initial reconcile runs builders, which may read captured signals.
        let _ambient = crate::reactive::ambient_guard(&runtime);
        let (mut arena, mut router) = (TreeArena::new(), SubscriberRouter::new());
        let root_key = root.key();
        // Join-only when the caller opened a build window (AppConfig::run spans the
        // factory and this reconcile under one generation); otherwise open and commit
        // our own, so the initial tree's registrations are sweepable by later reloads.
        // Standalone use with no window behaves exactly as before (untracked).
        let joined = runtime.borrow().build_window_open();
        if !joined {
            runtime.borrow_mut().begin_build_window();
        }
        let root_id = reconcile(&runtime, &mut arena, &mut router, None, root);
        if !joined {
            runtime.borrow_mut().end_build_window(true);
        }
        // The root's key has to be registered here as well as on re-mount, or a keyed root
        // would be unreachable from every key-based lookup while its own subtree worked.
        arena.bind_key(root_key, root_id);
        Self {
            scheduler: FrameScheduler::new(arena, router, runtime, root_id),
            scene: Scene::new(),
            logical_size: (width.max(1), height.max(1)),
            background: Color::WHITE,
            damage_ring: DamageRing::new(),
            pending_damage: DamageRegion::new(),
            frame_counter: 0,
            pending_preserved_canvas_rect: None,
            pointer: PointerInput::new(),
            queued_keys: Vec::new(),
            queued_wheels: Vec::new(),
            caret_blink_on: true,
            caret_last_seen: None,
            caret_epoch: 0.0,
            last_frame_error: None,
            hot_reload: HotReload::new(),
            staged_reload_error: None,
        }
    }

    /// Constructs an engine instance with a fresh reactive runtime.
    pub fn new(root_element: Element, width: u32, height: u32) -> Self {
        Self::with_runtime(
            Rc::new(RefCell::new(ReactiveRuntime::new())),
            root_element,
            width,
            height,
        )
    }

    /// Builds a headless engine preloaded with fallback typography fonts.
    pub fn headless(width: u32, height: u32) -> Self {
        let mut e = Self::new(crate::dsl::group().into_element(), width, height);
        e.load_font(include_bytes!("../../assets/inter-var.ttf"));
        e
    }

    /// Mounts a declarative element as the root of the UI tree.
    pub fn mount(&mut self, element: impl IntoElement) {
        self.set_root(element.into_element());
    }

    /// Allocates a reactive signal bound to this engine instance.
    pub fn signal<T: Clone + 'static>(&self, initial: T) -> Signal<T> {
        Signal::new(self.runtime(), initial)
    }

    /// Loads TrueType/OpenType font binary data into text layout context.
    pub fn load_font(&mut self, font_bytes: &[u8]) {
        self.scheduler.text_ctx.load_font(font_bytes);
    }

    /// Returns a clone handle to the active typography layout context.
    pub fn text_context(&self) -> TextContext {
        self.scheduler.text_ctx.clone()
    }

    /// Current logical resolution dimensions in pixels.
    #[inline(always)]
    pub fn logical_size(&self) -> (u32, u32) {
        self.logical_size
    }

    /// Configures the base background clear color.
    pub fn set_background(&mut self, color: Color) {
        self.background = color;
    }

    /// Current background clear color.
    #[inline(always)]
    pub fn background(&self) -> Color {
        self.background
    }

    /// Accesses the compiled display list scene from the most recent frame.
    #[inline(always)]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Mutably accesses the scene so extensions can inject overlay chunks.
    #[inline(always)]
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    /// Resizes engine viewport dimensions and records perimeter damage.
    ///
    /// A resize pushes the FULL new viewport as damage, not just the exposed
    /// strips. Resizing reallocates the backend surface, so no previously
    /// presented pixel can be trusted — a fresh buffer's validity is governed
    /// solely by the swapchain age, and a stale nonzero age would leave the
    /// "preserved" region unpainted (tear). One full repaint per resize event
    /// is the only safe assumption. This mirrors the backends' own convention
    /// (`WaylandBackend::needs_full`, vello's present-unconditionally-after-
    /// reconfigure) at the engine level so every present path agrees.
    /// `pending_preserved_canvas_rect` still reports the reusable overlap for
    /// diagnostics and for the renderer's canvas-pixel preservation, which is
    /// a separate concern from surface-buffer validity.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (nw, nh) = (width.max(1), height.max(1));
        if self.logical_size == (nw, nh) {
            return;
        }
        let (ow, oh) = self.logical_size;
        self.logical_size = (nw, nh);
        self.pending_preserved_canvas_rect = Some(ResolvedRect::new(
            0.0,
            0.0,
            ow.min(nw) as f32,
            oh.min(nh) as f32,
        ));
        self.pending_damage
            .push(ResolvedRect::new(0.0, 0.0, nw as f32, nh as f32));
    }

    /// Marks the full viewport damaged on the next frame without resizing.
    ///
    /// For presentation backends with retained canvases (a fresh backend
    /// after an engine/backend swap, a reconfigured surface): their pixels
    /// are undefined, so the next frame must repaint everything rather than
    /// only the incremental damage ring. Idempotent and cheap (one frame).
    pub fn request_full_repaint(&mut self) {
        let (w, h) = self.logical_size;
        self.pending_damage
            .push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
    }

    /// Replaces the declarative root node and marks layout dirty.
    ///
    /// A panicking rebuild does not propagate either: the old root is kept,
    /// the error is logged (and visible via [`Engine::last_frame_error`]),
    /// and a full repaint is scheduled. This is the remount half of the
    /// hot-reload story — `frame()` is the per-frame half.
    pub fn set_root(&mut self, element: Element) {
        let _ambient = crate::reactive::ambient_guard(&self.runtime());
        let key = element.key();
        // Join-only when a window is already open (reload drain, demo switch); otherwise
        // open and commit our own so remount registrations are sweepable by later reloads.
        // A panicking reconcile closes without committing (drops its partials).
        let joined = self.scheduler.runtime.borrow().build_window_open();
        if !joined {
            self.scheduler.runtime.borrow_mut().begin_build_window();
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reconcile(
                &self.scheduler.runtime,
                &mut self.scheduler.arena,
                &mut self.scheduler.router,
                Some(self.scheduler.root),
                element,
            )
        }));
        match outcome {
            Ok(root) => {
                if !joined {
                    self.scheduler.runtime.borrow_mut().end_build_window(true);
                }
                self.scheduler.arena.bind_key(key, root);
                self.scheduler.root = root;
                self.scheduler.arena.mark_dirty(root, DirtyFlags::LAYOUT);
                self.scheduler.dirty_nodes_this_frame.push(root);
            }
            Err(payload) => {
                if !joined {
                    self.scheduler.runtime.borrow_mut().end_build_window(false);
                }
                let msg = crate::dsl::panic_message(&payload);
                tracing::error!("aurora: set_root panicked ({msg}); keeping previous root");
                self.last_frame_error = Some(msg);
                let (w, h) = self.logical_size;
                self.pending_damage
                    .push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
            }
        }
    }

    /// Replaces the whole UI tree for demo/route switching.
    ///
    /// Hot-reload remounts (`set_root`) preserve hook state and tickers on
    /// purpose; a demo switch must not: the old tree's clocks, timers and
    /// memoized signals belong to a UI that no longer exists.
    ///
    /// Two-phase by necessity: component bodies run (and register tickers)
    /// while the replacement tree is *built*, so the epoch bump and hook
    /// clear happen in [`Self::prepare_switch`] *before* building, and this
    /// commits the built tree after. Bumping after the build would retire
    /// the new tree's own just-registered tickers (frozen animation).
    /// Demo-owned `AppExtension`s still need explicit swapping by the caller.
    pub fn switch_root(&mut self, element: Element) {
        // Closes the window `prepare_switch` opened: the incoming tree's registrations tag
        // with its generation, so this commit drops the dead tree's tagged registrations
        // alongside the epoch retire. Called without `prepare_switch`, this is a harmless
        // no-op close and the reconcile stays untracked, as before.
        self.scheduler.runtime.borrow_mut().end_build_window(true);
        self.set_root(element);
        self.request_full_repaint();
    }

    /// Begins a demo switch: retires older-epoch tickers and clears
    /// per-component hook state so the incoming tree mounts genuinely fresh
    /// (no recalled signals, no memoized clocks from a dead tree).
    ///
    /// Call before building the replacement tree, then commit with
    /// [`Self::switch_root`].
    pub fn prepare_switch(&mut self) {
        self.scheduler.runtime.borrow_mut().advance_ticker_epoch();
        self.scheduler.runtime.borrow_mut().clear_hook_state();
        // Opens the build window the boot builds under; `switch_root` closes it. The boot
        // runs component bodies (registering tickers) between the two calls, so the window
        // must span all three — opening here and closing there is the pairing. A stray open
        // window (a previous boot that never committed) is committed first so depth cannot
        // wedge: commit keeps its generation, never drops live registrations.
        if self.scheduler.runtime.borrow().build_window_open() {
            self.scheduler.runtime.borrow_mut().end_build_window(true);
        }
        self.scheduler.runtime.borrow_mut().begin_build_window();
    }

    /// Installs (or swaps) the root factory re-run on every hot-reload.
    ///
    /// `AppConfig::run` installs the app's root builder; a dylib host swaps in a fresh
    /// factory after its reloader reports a new library. See [`crate::hot_reload`].
    pub fn set_root_factory(&mut self, factory: SharedFactory) {
        self.hot_reload.set_root_factory(factory);
    }

    /// Queues a state-preserving full rebuild for the start of the next frame.
    ///
    /// The root factory re-runs under the same stable `"root"` hook scope, so every
    /// `use_signal`, `use_transition`, and ticker survives. The reconciler then patches
    /// only what changed. No-op without a factory (recorded in stats as a failure).
    pub fn queue_full_reload(&mut self) {
        self.hot_reload.queue_full_reload();
    }

    /// Queues a reload scoped to one stable component identity.
    ///
    /// Resolves to a root rebuild reported under that identity — the reconciler still
    /// patches only what changed, so scoping is about reporting, not cost.
    pub fn queue_component_reload(&mut self, instance: impl AsRef<str>) {
        self.hot_reload.queue_component_reload(instance);
    }

    /// Cumulative hot-reload telemetry.
    pub fn hot_reload_stats(&self) -> HotReloadStats {
        self.hot_reload.stats().clone()
    }

    /// Direct access to the hot-reload orchestrator (watcher registration, factory swap).
    pub fn hot_reload_mut(&mut self) -> &mut HotReload {
        &mut self.hot_reload
    }

    /// Direct read access to the hot-reload orchestrator.
    pub fn hot_reload(&self) -> &HotReload {
        &self.hot_reload
    }

    /// Replaces a single child slot of `parent` with a freshly built element.
    ///
    /// The surgical primitive behind component-scoped reloads: only the named slot is
    /// re-reconciled, so siblings keep their `NodeId`s, layout caches, and state. Panic-safe
    /// like [`Self::set_root`] — a panicking build keeps the previous subtree and records
    /// the error.
    pub fn set_root_at(&mut self, parent: NodeId, slot: usize, element: Element) {
        let _ambient = crate::reactive::ambient_guard(&self.runtime());
        // Join-or-open like `set_root`: remount registrations stay sweepable.
        let joined = self.scheduler.runtime.borrow().build_window_open();
        if !joined {
            self.scheduler.runtime.borrow_mut().begin_build_window();
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            replace_child(
                &self.scheduler.runtime,
                &mut self.scheduler.arena,
                &mut self.scheduler.router,
                parent,
                slot,
                element,
            )
        }));
        match outcome {
            Ok(_) => {
                if !joined {
                    self.scheduler.runtime.borrow_mut().end_build_window(true);
                }
                self.scheduler.dirty_nodes_this_frame.push(parent);
            }
            Err(payload) => {
                if !joined {
                    self.scheduler.runtime.borrow_mut().end_build_window(false);
                }
                let msg = crate::dsl::panic_message(&payload);
                tracing::error!("aurora: set_root_at panicked ({msg}); keeping previous subtree");
                self.last_frame_error = Some(msg);
                let (w, h) = self.logical_size;
                self.pending_damage
                    .push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
            }
        }
    }

    /// Drains hot-reload work at the start of a frame: build notes, watcher events, queued
    /// rebuilds. Runs before input dispatch so a reloaded tree sees this frame's passes.
    ///
    /// Returns the executed rebuild's report when one ran, with impact fields still zero:
    /// `frame()` fills them from the pass stats and records the report, so impact
    /// describes what the reload frame actually did.
    fn drain_hot_reload(&mut self) -> Option<ReloadReport> {
        self.hot_reload.drain_build_notes();
        let _watched_changed = self.hot_reload.drain_watcher();
        let Some(targets) = self.hot_reload.take_collapsed_pending() else {
            return None;
        };
        let t0 = std::time::Instant::now();
        let mut report = ReloadReport {
            reloaded: targets,
            duration: Duration::ZERO,
            error: None,
            laid_out: false,
            nodes_dirtied: 0,
            damage_rects: 0,
        };
        let Some(factory) = self.hot_reload.root_factory() else {
            report.error =
                Some("no root factory installed; call Engine::set_root_factory".to_string());
            report.duration = t0.elapsed();
            tracing::warn!(
                "aurora: hot-reload requested ({})",
                report.error.as_deref().unwrap_or("")
            );
            self.hot_reload.record_report(&report);
            return None;
        };
        let _ambient = crate::reactive::ambient_guard(&self.runtime());
        // Generation-scoped build window: everything the factory registers tags with this
        // generation. Commit keeps it and drops superseded twins; abort drops exactly the
        // failed build's registrations and keeps the live tree's.
        self.scheduler.runtime.borrow_mut().begin_build_window();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cx = Cx::new(self.scheduler.runtime.clone());
            cx.enter_keyed("root", || factory())
        }));
        match outcome {
            Ok(element) => {
                // Before/after IR for human diffing: the previous reload's tree
                // against this one's. DEBUG-gated (a tree walk per reload frame);
                // the per-node reconcile lines say what differed against retained
                // state, these two say what the factory produced.
                if tracing::enabled!(tracing::Level::DEBUG) {
                    tracing::debug!(
                        "aurora: hot-reload tree before:\n{}",
                        self.hot_reload
                            .last_tree_summary()
                            .unwrap_or("<first build>")
                    );
                    let after = element.summarize();
                    tracing::debug!("aurora: hot-reload tree after:\n{after}");
                    self.hot_reload.set_last_tree_summary(after);
                }
                self.set_root(element);
                self.scheduler.runtime.borrow_mut().end_build_window(true);
                report.duration = t0.elapsed();
                tracing::info!(
                    "aurora: hot-reload rebuilt [{}] in {:.1}ms",
                    report.reloaded.join(", "),
                    report.duration.as_secs_f64() * 1000.0
                );
                Some(report)
            }
            Err(payload) => {
                self.scheduler.runtime.borrow_mut().end_build_window(false);
                let msg = crate::dsl::panic_message(&payload);
                tracing::error!(
                    "aurora: hot-reload rebuild panicked ({msg}); keeping previous tree"
                );
                self.staged_reload_error = Some(msg.clone());
                report.error = Some(msg);
                report.duration = t0.elapsed();
                self.hot_reload.record_report(&report);
                let (w, h) = self.logical_size;
                self.pending_damage
                    .push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
                None
            }
        }
    }

    /// Runs the frame passes, committing bounds and compiling the display list scene.
    ///
    /// A panicking pass does not propagate: the boundary catches it, keeps the
    /// last good scene (callers present what `scene()` still holds), records
    /// the error for [`Engine::last_frame_error`], and schedules a full
    /// repaint so the next clean frame redraws from a consistent tree. The
    /// scene may be half-rebuilt at the catch point — presenting it would
    /// flash a partial frame, so callers should skip presenting a frame whose
    /// report follows an error. `RefCell`-based runtime state is not poisoned
    /// by unwinds, and no `borrow_mut` is held across the boundary: the
    /// `&mut` scheduler reborrow ends as the stack unwinds.
    #[tracing::instrument(skip_all, fields(w = self.logical_size.0, h = self.logical_size.1))]
    pub fn frame(&mut self) -> FrameReport {
        // No mutable borrow may be held across this boundary: a component body that
        // panics while one is held would poison every later frame with
        // `BorrowMutError`. Debug-only tripwire; release pays nothing.
        debug_assert!(
            self.scheduler.runtime.try_borrow_mut().is_ok(),
            "runtime mutably borrowed across frame boundary"
        );
        // Ambient for everything a frame runs: tickers, effects, post-layout
        // callbacks, scheduler rebuilds, and component bodies. Nested guards
        // (dispatch pushes its own below) resolve to the same runtime.
        let _ambient = crate::reactive::ambient_guard(&self.runtime());
        let (w, h) = self.logical_size;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Hot-reload rebuilds land before anything else: the new tree is reconciled first,
        // so this frame's tick, input, react, and layout passes all see it at once. A
        // returned report is completed from the pass stats below and recorded there.
        let pending_report = self.drain_hot_reload();

        // The clock ticks before input dispatch, so queued events advance
        // against this frame's own deltas: pointer velocity measures the
        // frame it arrives in, and the click burst sees an idle stretch in
        // full on the very first frame back rather than one frame late.
        self.scheduler.clock.tick();
        // Pointer handlers run before any pass so a write they make is picked up by this
        // frame's layout and damage, rather than landing one frame late.
        self.dispatch_queued_events();
        // The caret blinks on animation time (~2Hz): repaint the focused caret
        // only when the phase flips, so a static highlight costs no frames.
        self.poll_caret_blink();
        let frame_damage = self.damage_ring.advance();
        for r in self.pending_damage.rects() {
            frame_damage.push(*r);
        }
        self.pending_damage.clear();

        let root = self.scheduler.root;
        let caret_visible = self.caret_blink_on;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_frame_passes(
                &mut self.scheduler,
                root,
                &mut self.scene,
                w,
                h,
                self.frame_counter,
                frame_damage,
                self.pending_preserved_canvas_rect.take(),
                caret_visible,
            )
        }));
        let (stats, diag) = match outcome {
            Ok(ok) => {
                self.last_frame_error = None;
                ok
            }
            Err(payload) => {
                let msg = crate::dsl::panic_message(&payload);
                tracing::error!(
                    "aurora: frame {} panicked ({msg}); keeping last good scene",
                    self.frame_counter
                );
                self.last_frame_error = Some(msg);
                // The tree may hold partial pass state; repaint everything
                // next frame so recovery starts from a consistent tree.
                self.pending_damage
                    .push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
                Default::default()
            }
        };
        // A failed hot-reload rebuild stages its error here so it survives the clean pass
        // above and stays visible for exactly one presented frame.
        if let Some(msg) = self.staged_reload_error.take() {
            self.last_frame_error = Some(msg);
        }
        // A panic inside the passes must not have leaked a borrow: fail fast in debug
        // tests instead of cascading `BorrowMutError` through later frames.
        debug_assert!(
            self.scheduler.runtime.try_borrow_mut().is_ok(),
            "frame passes leaked a runtime borrow across unwind"
        );
        // Complete a reload executed at frame start with what the passes actually did.
        // A panicking pass leaves zeros (the tree state is suspect; impact is meaningless).
        if let Some(mut report) = pending_report {
            if self.last_frame_error.is_none() {
                report.laid_out = stats.laid_out;
                report.nodes_dirtied = stats.nodes_dirtied;
                report.damage_rects = diag.spatial.damaged_rects.len();
            }
            // One line per reload frame saying what the engine actually did with the
            // new tree: `new_cmds`/`cached` tells whether compile re-emitted content
            // or replayed retained chunks; `damage` tells whether the repaint was
            // even requested. A changed fill with `new_cmds=0` means the scene kept
            // stale commands; `damage=0` means the backend was never asked.
            tracing::info!(
                "aurora: hot-reload frame [{}] laid_out={} dirtied={} damage={} \
                 new_cmds={} cached_chunks={} in {:.1}ms{}",
                report.reloaded.join(","),
                report.laid_out,
                report.nodes_dirtied,
                report.damage_rects,
                diag.compile.commands_emitted,
                diag.compile.cached_chunks,
                report.duration.as_secs_f64() * 1000.0,
                report
                    .error
                    .as_deref()
                    .map(|e| format!(" error={e}"))
                    .unwrap_or_default(),
            );
            self.hot_reload.record_report(&report);
        }

        FrameReport {
            stats,
            layers: self.collect_dirtied_layers(),
            diagnostics: diag,
        }
    }

    /// Panic message from the last frame that unwound, if any.
    ///
    /// `Some` means the most recent `frame()` call caught a panic and kept the
    /// previous scene: skip presenting and show this instead (the HUD reads
    /// it). Cleared by the next clean frame.
    pub fn last_frame_error(&self) -> Option<&str> {
        self.last_frame_error.as_deref()
    }

    /// Queues a key press for dispatch at the start of the next frame.
    pub fn queue_key_event(&mut self, event: KeyEvent) {
        self.queued_keys.push(event);
    }

    /// Queues a wheel scroll for dispatch at the start of the next frame.
    pub fn queue_wheel_event(&mut self, event: WheelInfo) {
        self.queued_wheels.push(event);
    }

    /// Cursor shape for the platform: I-beam while hovering selectable text.
    pub fn cursor_icon(&self) -> CursorIcon {
        self.pointer.router().cursor_icon(&self.scheduler.arena)
    }

    /// Current selection on a keyed text node, if it holds one.
    pub fn text_selection(&self, key: impl Into<Key>) -> Option<TextSelection> {
        self.with_arena(|arena, _| {
            let id = arena.node_for_key(&key.into())?;
            arena.get(id).state.text.text_selection
        })
    }

    /// Selected substring on a keyed text node, empty when collapsed or absent.
    pub fn selected_text(&self, key: impl Into<Key>) -> Option<String> {
        self.with_arena(|arena, _| {
            let id = arena.node_for_key(&key.into())?;
            let node = arena.get(id);
            let text = match &node.kind {
                crate::tree::NodeKind::Text(t) => t.content.clone(),
                _ => return None,
            };
            let sel = node.state.text.text_selection?;
            let (s, e) = sel.normalized();
            Some(text.get(s..e).unwrap_or("").to_string())
        })
    }

    /// Selected substrings across all painted runs, in document order.
    ///
    /// Single-run selections yield one entry; cross-run drags yield one entry
    /// per covered run (edge runs clipped to their sub-range, interior runs
    /// whole). Empty when nothing is highlighted.
    pub fn combined_selected_text(&self) -> Vec<String> {
        self.with_arena(|arena, root| {
            let mut out = Vec::new();
            let mut stack = vec![root];
            // Preorder DFS with reversed pushes keeps document order without
            // allocating a child list per node.
            while let Some(id) = stack.pop() {
                if !arena.is_valid(id) {
                    continue;
                }
                {
                    let node = arena.get(id);
                    if let crate::tree::NodeKind::Text(t) = &node.kind {
                        if t.selectable {
                            if let Some(sel) = node.state.text.text_selection {
                                let (s, e) = sel.normalized();
                                if e > s {
                                    out.push(t.content.get(s..e).unwrap_or("").to_string());
                                }
                            }
                        }
                    }
                }
                let children = arena.children(id);
                for &child in children.iter().rev() {
                    stack.push(child);
                }
            }
            out
        })
    }

    /// Programmatically sets (or clears) a keyed text node's selection.
    ///
    /// Offsets snap to character boundaries; the node repaints but never
    /// re-lays out. Returns false when the key resolves to no text node.
    pub fn set_text_selection(
        &mut self,
        key: impl Into<Key>,
        selection: Option<TextSelection>,
    ) -> bool {
        let id = self.with_arena(|arena, _| arena.node_for_key(&key.into()));
        let Some(id) = id else { return false };
        let node = self.scheduler.arena.get(id);
        if !matches!(&node.kind, crate::tree::NodeKind::Text(_)) {
            return false;
        }
        let node = self.scheduler.arena.get_mut(id);
        node.state.text.text_selection = selection;
        self.scheduler.arena.mark_dirty(id, DirtyFlags::PAINT);
        true
    }

    /// Key of the text node owning keyboard focus, if any.
    pub fn focused_text(&self) -> Option<Key> {
        self.with_arena(|arena, _| {
            let id = self.pointer.router().focused()?;
            if !arena.is_valid(id) {
                return None;
            }
            arena.get(id).key.clone()
        })
    }

    /// Whether a key press would reach anything: focused selectable text or a
    /// focused node with a key handler (e.g. a scroll viewport). The bridge
    /// uses this (not `focused_text`) to decide whether forwarding a key can
    /// change anything.
    pub fn has_keyboard_target(&self) -> bool {
        self.with_arena(|arena, _| {
            let Some(id) = self.pointer.router().focused() else {
                return false;
            };
            if !arena.is_valid(id) {
                return false;
            }
            let node = arena.get(id);
            node.state.input.handlers.get().is_some_and(|h| h.key.is_some())
                || matches!(&node.kind, crate::tree::NodeKind::Text(t) if t.selectable)
        })
    }

    /// Last text copied with Ctrl+C, if any (internal clipboard).
    pub fn clipboard_text(&self) -> Option<String> {
        self.pointer.router().clipboard().map(str::to_string)
    }

    /// Queues a pointer event for dispatch at the start of the next frame.
    pub fn queue_pointer_event(&mut self, event: PointerEvent) {
        self.pointer.queue(event);
    }

    /// Convenience for a press at a point.
    pub fn queue_pointer_down(&mut self, x: f32, y: f32) {
        self.pointer.queue_down(x, y);
    }

    /// Convenience for a release at a point.
    pub fn queue_pointer_up(&mut self, x: f32, y: f32) {
        self.pointer.queue_up(x, y);
    }

    /// Convenience for a pointer move to a point, classified as drag or hover by the router.
    pub fn queue_pointer_move(&mut self, x: f32, y: f32) {
        self.pointer.queue_move(x, y);
    }

    /// Pointer position the engine last saw, absent while the pointer is outside the window.
    pub fn pointer_position(&self) -> Option<Point> {
        self.pointer.position()
    }

    /// Whether the pointer has travelled far enough past a press to count as a drag.
    ///
    /// Diagnostics, and how a test can tell a recognised gesture from an armed one: a press
    /// alone is not a drag, and treating it as one is how "the card is stuck to the cursor"
    /// bugs get introduced.
    pub fn pointer_is_dragging(&self) -> bool {
        self.pointer.is_dragging()
    }

    /// Releases any gesture the engine is holding, for instance when the window loses focus
    /// or the pointer leaves the surface.
    ///
    /// This gives up the capture as well as the hover. A drag that is only half-released keeps
    /// sending moves to the view it was pressed on, so the view follows a pointer that no
    /// longer exists.
    pub fn cancel_pointer_gesture(&mut self) {
        self.pointer.cancel_gesture();
    }

    /// Drains queued pointer events, returning the state left behind by the last one.
    fn dispatch_queued_events(&mut self) -> Option<PointerOutcome> {
        let last_delta = self.scheduler.clock.last_delta();
        let wall_delta = self.scheduler.clock.wall_delta();
        let outcome = self
            .pointer
            .dispatch(&mut self.scheduler, last_delta, wall_delta);
        // Keys run after the pointer queue so a press that moves focus lands
        // before the key that uses it. Wheels run with them: position-routed,
        // needing no focus, but sharing the frame boundary for signal writes.
        for key in std::mem::take(&mut self.queued_keys) {
            self.pointer
                .router_mut()
                .dispatch_key(&mut self.scheduler.arena, &key);
        }
        for wheel in std::mem::take(&mut self.queued_wheels) {
            self.pointer.router_mut().dispatch_wheel(
                &mut self.scheduler.arena,
                self.scheduler.root,
                &wheel,
            );
        }
        outcome
    }

    /// Key-based reads of node geometry, in explicit coordinate spaces.
    pub fn nodes(&self) -> NodeQuery<'_> {
        NodeQuery::new(&self.scheduler)
    }

    /// Reads the render box of a keyed view: where it is actually drawn right now.
    ///
    /// Differs from [`Self::rect`] while layout motion is animating, which is exactly
    /// when a caller wants to inspect the moving box rather than the settled one.
    pub fn render_rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        self.nodes().render_rect(key)
    }

    /// Reads the laid-out rect of a keyed view in its parent's coordinate space.
    pub fn rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        self.nodes().flow_rect(key)
    }

    /// Reads the transform a keyed view is currently painted with.
    ///
    /// This is the composed result of declared style, the reactive pose components, and any
    /// compositor override, so it is what a caller should compare against when a view is
    /// driven by an animated transform rather than by layout.
    pub fn transform(&self, key: impl Into<Key>) -> Option<Transform> {
        self.nodes().transform(key)
    }

    /// Reads a keyed view's box in absolute window coordinates.
    ///
    /// [`Self::rect`] is parent-local, which is what layout wants but not what a pointer
    /// wants: anything positioning a cursor, a hit test, or an overlay needs the position
    /// the view is actually drawn at.
    pub fn absolute_rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        self.nodes().absolute_rect(key)
    }

    /// Sets the rate time runs at: `0.0` freezes the engine, `0.1` is slow motion.
    ///
    /// Applies to every time-driven system at once — compositor motion, reactive tickers,
    /// render-box springs — which is what makes it usable for inspecting a transition
    /// mid-flight rather than only for slowing the whole app down.
    pub fn set_time_scale(&mut self, scale: f32) {
        self.scheduler.clock.set_time_scale(scale);
    }

    /// Scaled delta the last frame produced, for diagnostics.
    pub fn clock_debug_last_delta(&self) -> f32 {
        self.diagnostics().last_delta
    }

    /// Asks for exactly one animation quantum to run on the next frame, whatever the time scale.
    ///
    /// This is what makes a frozen engine inspectable one frame at a time. Freezing alone shows
    /// one frame forever, and forcing a delta cannot help because a forced delta is scaled like
    /// any other.
    pub fn step_animation(&mut self) {
        self.scheduler.clock.request_step();
    }

    /// Whether a requested step is still waiting to run, for diagnostics.
    pub fn animation_step_pending(&self) -> bool {
        self.diagnostics().animation_step_pending
    }

    /// Total animation time simulated so far, in seconds.
    ///
    /// Frame-rate independent by construction, so it is the only way to compare two runs that
    /// used different frame rates.
    pub fn clock_debug_simulated(&self) -> f32 {
        self.diagnostics().simulated
    }

    /// Time banked toward the next animation quantum, for diagnostics.
    pub fn clock_debug_accumulated(&self) -> f32 {
        self.diagnostics().accumulated
    }

    /// The current time scale.
    pub fn time_scale(&self) -> f32 {
        self.diagnostics().time_scale
    }

    /// Runs a read-only closure over the retained tree.
    ///
    /// Diagnostics need to walk nodes the public key API cannot reach — every sibling, every
    /// unclipped overflow, every ancestor chain. This hands out an immutable view rather than
    /// the arena itself, so a diagnostic cannot mutate the tree behind the engine's back.
    pub fn with_arena<R>(&self, visit: impl FnOnce(&TreeArena, NodeId) -> R) -> R {
        self.nodes().with_arena(visit)
    }

    /// Forces the next frame delta so headless tests can step time deterministically.
    pub fn advance(&mut self, dt: f32) {
        self.scheduler.clock.force(dt);
    }

    /// Whether any animation still wants frames.
    ///
    /// Reactive tickers, plus one engine-owned case: a focused caret blinks,
    /// so its frame loop runs until the caret collapses away or blurs. A
    /// static highlight needs no frames and keeps none alive.
    pub fn has_active_motion(&self) -> bool {
        self.scheduler.runtime.borrow().has_active_tickers()
            || self
                .pointer
                .router()
                .has_visible_caret(&self.scheduler.arena)
    }

    /// Blink phase from the last caret activity (~2Hz, frozen with the clock).
    ///
    /// Read by [`Self::poll_caret_blink`] and passed to
    /// [`run_frame_passes`](crate::runtime::frame::run_frame_passes) so the
    /// caret emits only while the phase is on.
    fn caret_phase(&self) -> bool {
        ((self.scheduler.clock.simulated() - self.caret_epoch) * 2.0).floor() as u64 % 2 == 0
    }

    /// Repaints the focused caret when its blink phase flips.
    ///
    /// Only flips dirty the node; idle frames between flips still run (see
    /// [`Self::has_active_motion`]) but change nothing. Any caret change —
    /// move, collapse, refocus — restarts the blink visibly on, like a
    /// browser showing the caret the moment you act.
    fn poll_caret_blink(&mut self) {
        let caret = self.pointer.router().focused().and_then(|id| {
            if !self.scheduler.arena.is_valid(id) {
                return None;
            }
            let sel = self.scheduler.arena.get(id).state.text.text_selection?;
            sel.is_collapsed().then_some((id, sel))
        });
        // A new or moved caret shows immediately with a full on-period; the
        // blink resumes after.
        if caret != self.caret_last_seen {
            self.caret_last_seen = caret;
            if caret.is_some() {
                tracing::debug!(?caret, "caret shown, blink restarted");
                self.caret_epoch = self.scheduler.clock.simulated();
                self.caret_blink_on = true;
                if let Some((focused, _)) = caret {
                    self.scheduler.arena.mark_dirty(focused, DirtyFlags::PAINT);
                }
            }
            return;
        }
        if caret.is_some() {
            let phase = self.caret_phase();
            if phase != self.caret_blink_on {
                self.caret_blink_on = phase;
                if let Some((focused, _)) = caret {
                    self.scheduler.arena.mark_dirty(focused, DirtyFlags::PAINT);
                }
            }
        } else {
            self.caret_blink_on = true;
        }
    }

    /// A consistent snapshot of every internal counter, for tests and debug tooling.
    ///
    /// The individual accessors above all read through this, so there is one derivation point
    /// rather than a dozen that can disagree.
    pub fn diagnostics(&self) -> EngineDiagnostics {
        EngineDiagnostics {
            last_delta: self.scheduler.clock.last_delta(),
            simulated: self.scheduler.clock.simulated(),
            accumulated: self.scheduler.clock.accumulated(),
            time_scale: self.scheduler.clock.time_scale(),
            animation_step_pending: self.scheduler.clock.step_pending(),
        }
    }

    /// Resolves a window point to the topmost keyed view under it.
    ///
    /// Deliberately the same code path the router uses for real pointer events. This used to be a
    /// second implementation that ignored ancestor clips and used document order, so it disagreed
    /// with the router on clipped and `z_index`-stacked views -- and this is the entry point tests
    /// aim with, so the disagreement sent them at the wrong view.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Key> {
        self.nodes().hit_test(x, y)
    }

    /// Accesses the active damaged boundary regions for the current frame.
    #[inline(always)]
    pub fn current_damage(&self) -> &DamageRegion {
        self.damage_ring.current()
    }

    /// Computes accumulated damaged rectangles across swapchain buffer age.
    #[inline(always)]
    pub fn damage_for_age(&self, age: u8) -> DamageRegion {
        let (w, h) = self.logical_size;
        self.damage_ring
            .damage_for_age(age, ResolvedRect::new(0.0, 0.0, w as f32, h as f32))
    }

    /// Identifies all layer-backed subtrees mutated during the current frame.
    pub fn dirtied_layers(&self) -> Vec<LayerId> {
        self.collect_dirtied_layers()
    }

    fn collect_dirtied_layers(&self) -> Vec<LayerId> {
        let mut layers = Vec::new();
        for &node in &self.scheduler.dirty_nodes_this_frame {
            layers.push(LayerId::from_packed(node.packed()));
            let mut curr = node;
            while let Some(parent) = self.scheduler.arena.parent(curr) {
                if self.scheduler.arena.get(parent).style.has_layer {
                    layers.push(LayerId::from_packed(parent.packed()));
                }
                curr = parent;
            }
        }
        layers.sort_unstable_by_key(|l| l.0);
        layers.dedup();
        layers
    }

    /// Accesses the underlying reactive execution runtime handle.
    pub fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        Rc::clone(&self.scheduler.runtime)
    }

    /// Runs `f` with this engine's runtime ambient.
    ///
    /// Needed for signal access outside driven regions (frames, dispatch, mounts):
    /// test assertions, setup writes, event-loop callbacks. Inside driven regions
    /// the engine pushes its own guard, so this is only for external code — and
    /// only when another engine was active more recently on this thread.
    pub fn guard<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = crate::reactive::ambient_guard(&self.runtime());
        f()
    }

    /// Builds a component context bound to this engine's runtime.
    pub fn cx(&self) -> Cx {
        Cx::new(self.runtime())
    }
}
