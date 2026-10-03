// Single responsibility: Encapsulated UI engine orchestrating layout, scene compilation, and invalidations.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dsl::element::Element;
use crate::dsl::reconciler::reconcile;
use crate::dsl::IntoElement;
use crate::foundation::{Color, DamageRegion, DamageRing, ResolvedRect};
use crate::reactive::{ReactiveRuntime, Signal};
use crate::runtime::frame::execute_frame_stages;
use crate::runtime::router::SubscriberRouter;
use crate::runtime::scheduler::{FrameScheduler, FrameStats};
use crate::runtime::FrameDiagnostics;
use crate::scene::{LayerId, Scene};
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, TreeArena};

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
        self.root = reconcile(
            &self.scheduler.runtime,
            &mut self.scheduler.arena,
            &mut self.scheduler.router,
            Some(self.root),
            element,
        );
        self.scheduler.root = self.root;
        self.scheduler
            .arena
            .mark_dirty(self.root, DirtyFlags::LAYOUT);
        self.scheduler.dirty_nodes_this_frame.push(self.root);
    }

    /// Executes stages 1-5, committing bounds and compiling display list scene.
    #[tracing::instrument(skip_all, fields(w = self.logical_size.0, h = self.logical_size.1))]
    pub fn frame(&mut self) -> (FrameStats, Vec<LayerId>, FrameDiagnostics) {
        let (w, h) = self.logical_size;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let frame_damage = self.damage_ring.advance();
        for r in self.pending_damage.rects() {
            frame_damage.push(*r);
        }
        self.pending_damage.clear();

        let (stats, diag) = execute_frame_stages(
            &mut self.scheduler,
            self.root,
            &mut self.scene,
            w,
            h,
            self.frame_counter,
            frame_damage,
            self.pending_preserved_canvas_rect.take(),
        );

        (stats, self.collect_dirtied_layers(), diag)
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
            layers.push(LayerId::from(node));
            let mut curr = node;
            while let Some(parent) = self.scheduler.arena.parent(curr) {
                if self.scheduler.arena.get(parent).style.has_layer {
                    layers.push(LayerId::from(parent));
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
