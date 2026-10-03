// Single responsibility: Encapsulated UI engine orchestrating layout, scene compilation, and invalidations.

use std::cell::RefCell;
use std::rc::Rc;
use tiny_skia::Pixmap;

use crate::dsl::element::Element;
use crate::dsl::reconciler::reconcile;
use crate::dsl::IntoElement;
use crate::foundation::{Color, DamageRegion, DamageRing, Key, ResolvedRect};
use crate::motion::{self, AnimationBuilder, Controller, MotionState, Spring};
use crate::reactive::{ReactiveRuntime, Signal};
use crate::runtime::frame::run_frame_passes;
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
    root: NodeId,
    logical_size: (u32, u32),
    background: Color,
    damage_ring: DamageRing,
    pending_damage: DamageRegion,
    frame_counter: u64,
    pending_preserved_canvas_rect: Option<ResolvedRect>,
    raster: HeadlessRaster,
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
        let root_id = reconcile(&runtime, &mut arena, &mut router, None, root);
        Self {
            scheduler: FrameScheduler::new(arena, router, runtime, root_id),
            scene: Scene::new(),
            root: root_id,
            logical_size: (width.max(1), height.max(1)),
            background: Color::WHITE,
            damage_ring: DamageRing::new(),
            pending_damage: DamageRegion::new(),
            frame_counter: 0,
            pending_preserved_canvas_rect: None,
            raster: HeadlessRaster::new(),
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
        self.root = reconcile(
            &self.scheduler.runtime,
            &mut self.scheduler.arena,
            &mut self.scheduler.router,
            Some(self.root),
            element,
        );
        self.scheduler.arena.bind_key(key, self.root);
        self.scheduler.root = self.root;
        self.scheduler
            .arena
            .mark_dirty(self.root, DirtyFlags::LAYOUT);
        self.scheduler.dirty_nodes_this_frame.push(self.root);
    }

    /// Runs the frame passes, committing bounds and compiling the display list scene.
    #[tracing::instrument(skip_all, fields(w = self.logical_size.0, h = self.logical_size.1))]
    pub fn frame(&mut self) -> FrameReport {
        let (w, h) = self.logical_size;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let frame_damage = self.damage_ring.advance();
        for r in self.pending_damage.rects() {
            frame_damage.push(*r);
        }
        self.pending_damage.clear();

        let (stats, diag) = run_frame_passes(
            &mut self.scheduler,
            self.root,
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

    /// Applies a motion override to a keyed view, invalidating paint only.
    pub fn update_motion(
        &mut self,
        key: impl Into<Key>,
        update: impl FnOnce(&mut MotionState),
    ) -> bool {
        motion::apply::apply_motion(&mut self.scheduler, &key.into(), update)
    }

    /// Starts an animation against a keyed view, retargeting any motion already running.
    pub fn animate(&mut self, key: impl Into<Key>) -> AnimationBuilder<'_> {
        AnimationBuilder::new(&mut self.scheduler, key.into())
    }

    /// Starts a timeline scheduling several keyed animations against one clock.
    pub fn timeline(&mut self) -> motion::Timeline<'_> {
        motion::Timeline::new(&mut self.scheduler)
    }

    /// Reads the current motion overrides of a keyed view.
    pub fn motion_state(&self, key: impl Into<Key>) -> Option<MotionState> {
        let key = key.into();
        let id = self.scheduler.arena.node_for_key(&key)?;
        Some(self.scheduler.arena.get(id).state.motion)
    }

    /// Reads the laid-out rect of a keyed view in its parent's coordinate space.
    pub fn rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        let id = self.scheduler.arena.node_for_key(&key.into())?;
        Some(self.scheduler.arena.get(id).resolved_rect)
    }

    /// Mutably reaches the controller currently animating a keyed view.
    pub fn controller(&mut self, key: impl Into<Key>) -> Option<&mut Controller> {
        self.scheduler.motion.controller_mut(&key.into())
    }

    /// Forces the next frame delta so headless tests can step time deterministically.
    pub fn advance(&mut self, dt: f32) {
        self.scheduler.motion.force_delta(dt);
    }

    /// Whether any controller still wants frames.
    pub fn has_active_motion(&self) -> bool {
        self.scheduler.motion.is_active()
    }

    /// Resolves a window point to the topmost keyed view under it.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Key> {
        crate::tree::hit_test::hit_test(
            &self.scheduler.arena,
            self.scheduler.root,
            crate::foundation::Point::new(x, y),
        )
    }

    /// Publishes a spring's position as a signal settling from `from` toward `to`.
    pub fn spring_signal(&mut self, spring: Spring, from: f32, to: f32) -> Signal<f32> {
        let signal = Signal::new(Rc::clone(&self.scheduler.runtime), from);
        self.scheduler
            .motion
            .publish_spring(signal.clone(), spring, from, to);
        signal
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
}
