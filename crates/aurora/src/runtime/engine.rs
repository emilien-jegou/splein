// Single responsibility: Encapsulated UI engine orchestrating layout, scene compilation, and invalidations.

use std::cell::RefCell;
use std::rc::Rc;
use tiny_skia::Pixmap;

use crate::dsl::element::Element;
use crate::dsl::reconciler::reconcile;
use crate::dsl::IntoElement;
use crate::foundation::{Color, DamageRegion, DamageRing, Key, Point, ResolvedRect, Transform};
use crate::input::{PointerEvent, PointerOutcome};
use crate::reactive::{Cx, ReactiveRuntime, Signal};
use crate::runtime::diagnostics::EngineDiagnostics;
use crate::runtime::frame::run_frame_passes;
use crate::runtime::node_query::NodeQuery;
use crate::runtime::pointer_input::PointerInput;
use crate::runtime::raster::{HeadlessFrame, HeadlessRaster};
use crate::runtime::scheduler::FrameScheduler;
use crate::runtime::FrameReport;
use crate::scene::{LayerId, Scene};
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, SubscriberRouter, TreeArena};

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
    raster: HeadlessRaster,
    /// Pointer routing: hit resolution, press capture, gesture recognition, and the queue
    /// of events waiting for the next frame.
    pointer: PointerInput,
}

impl Engine {
    /// Creates an engine instance configured with an existing reactive runtime.
    pub fn with_runtime(
        runtime: Rc<RefCell<ReactiveRuntime>>,
        root: Element,
        width: u32,
        height: u32,
    ) -> Self {
        let (mut arena, mut router) = (TreeArena::new(), SubscriberRouter::new());
        let root_key = root.key();
        let root_id = reconcile(&runtime, &mut arena, &mut router, None, root);
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
            raster: HeadlessRaster::new(),
            pointer: PointerInput::new(),
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

        if nw > ow {
            self.pending_damage.push(ResolvedRect::new(
                ow as f32,
                0.0,
                (nw - ow) as f32,
                oh.min(nh) as f32,
            ));
        }
        if nh > oh {
            self.pending_damage.push(ResolvedRect::new(
                0.0,
                oh as f32,
                nw as f32,
                (nh - oh) as f32,
            ));
        }
    }

    /// Replaces the declarative root node and marks layout dirty.
    pub fn set_root(&mut self, element: Element) {
        let key = element.key();
        let root = reconcile(
            &self.scheduler.runtime,
            &mut self.scheduler.arena,
            &mut self.scheduler.router,
            Some(self.scheduler.root),
            element,
        );
        self.scheduler.arena.bind_key(key, root);
        self.scheduler.root = root;
        self.scheduler.arena.mark_dirty(root, DirtyFlags::LAYOUT);
        self.scheduler.dirty_nodes_this_frame.push(root);
    }

    /// Runs the frame passes, committing bounds and compiling the display list scene.
    #[tracing::instrument(skip_all, fields(w = self.logical_size.0, h = self.logical_size.1))]
    pub fn frame(&mut self) -> FrameReport {
        let (w, h) = self.logical_size;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Pointer handlers run before any pass so a write they make is picked up by this
        // frame's layout and damage, rather than landing one frame late.
        self.dispatch_queued_events();
        let frame_damage = self.damage_ring.advance();
        for r in self.pending_damage.rects() {
            frame_damage.push(*r);
        }
        self.pending_damage.clear();

        let root = self.scheduler.root;
        let (stats, diag) = run_frame_passes(
            &mut self.scheduler,
            root,
            &mut self.scene,
            w,
            h,
            self.frame_counter,
            frame_damage,
            self.pending_preserved_canvas_rect.take(),
        );

        FrameReport {
            stats,
            layers: self.collect_dirtied_layers(),
            diagnostics: diag,
        }
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
        self.pointer.dispatch(&self.scheduler, last_delta)
    }

    /// Rasterizes the compiled scene's damaged region into a persistent CPU canvas.
    pub fn canvas(&mut self) -> &Pixmap {
        let size = self.logical_size();
        let text_ctx = self.text_context();
        let damage = self.current_damage().clone();
        self.raster.draw(HeadlessFrame {
            scene: &self.scene,
            damage: &damage,
            size,
            text_ctx: &text_ctx,
        })
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
    /// Now purely a question about reactive tickers: the engine owns no animation of its own, so
    /// a frame is worth drawing only while a userland ticker is still settling toward a target.
    /// This is what keeps the redraw loop running for a client-driven transition and lets it
    /// stop once that transition has settled.
    pub fn has_active_motion(&self) -> bool {
        self.scheduler.runtime.borrow().has_active_tickers()
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

    /// Builds a component context bound to this engine's runtime.
    pub fn cx(&self) -> Cx {
        Cx::new(self.runtime())
    }
}
