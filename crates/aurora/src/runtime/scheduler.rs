// Single responsibility: Multi-phase frame execution, metrics reporting, and reactive queue drainage.

use smallvec::SmallVec;
use std::cell::RefCell;
use std::rc::Rc;

use aurora_anim_core::clock::Clock;
use aurora_foundation::{Constraints, Key, Point};
use crate::layout::{layout_node_with_text, LayoutResult};
use aurora_reactive::{Observer, ReactiveRuntime, SubscriberId};
use crate::runtime::boundary::{collect_layout_boundaries, prune_nested_boundaries};
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, NodeKind, SubscriberRouter, TreeArena};

/// Execution summary of reactive updates and layout recalculations.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub flags: DirtyFlags,
    pub nodes_dirtied: usize,
    pub laid_out: bool,
    pub drain_passes: usize,
    pub paint_passes: usize,
}

struct ObserverGuard<'a> {
    runtime: &'a RefCell<ReactiveRuntime>,
}

impl<'a> ObserverGuard<'a> {
    #[inline(always)]
    fn new(runtime: &'a RefCell<ReactiveRuntime>, observer: Observer) -> Self {
        runtime.borrow_mut().push_observer(observer);
        Self { runtime }
    }
}

impl<'a> Drop for ObserverGuard<'a> {
    #[inline(always)]
    fn drop(&mut self) {
        self.runtime.borrow_mut().pop_observer();
    }
}

/// Frame scheduler coordinating reactive drainage, layout boundaries, and layout caches.
pub struct FrameScheduler {
    pub arena: TreeArena,
    pub router: SubscriberRouter,
    pub runtime: Rc<RefCell<ReactiveRuntime>>,
    pub text_ctx: TextContext,
    pub root: NodeId,
    pub dirty_nodes_this_frame: SmallVec<[NodeId; 16]>,
    /// Single frame clock feeding the reactive tickers that drive userland animation.
    pub clock: Clock,
    prev_constraints: Option<Constraints>,
}

impl FrameScheduler {
    pub fn new(
        arena: TreeArena,
        router: SubscriberRouter,
        runtime: Rc<RefCell<ReactiveRuntime>>,
        root: NodeId,
    ) -> Self {
        Self {
            arena,
            router,
            runtime,
            text_ctx: TextContext::new(),
            root,
            dirty_nodes_this_frame: SmallVec::new(),
            clock: Clock::new(),
            prev_constraints: None,
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn commit_frame(
        &mut self,
        window_constraints: Constraints,
        layout_result: &mut LayoutResult,
    ) -> FrameStats {
        self.dirty_nodes_this_frame.clear();
        let mut external_flags = DirtyFlags::NONE;
        // Every mark since the last commit — reactive bindings and direct
        // input/engine writes alike — joins the frame's dirty set. Direct
        // writes (selection highlights, focus flags) bypass the subscriber
        // drains below, so without this their nodes never reach the damage
        // plan and paint-only changes never rasterize.
        for (node, flags) in std::mem::take(&mut self.arena.dirty_outbox) {
            self.dirty_nodes_this_frame.push(node);
            external_flags |= flags;
        }

        let mut frame_flags = self.check_constraint_change(window_constraints);
        let (react_flags, drain_passes) = self.drain_reactive_mutations();
        frame_flags |= react_flags;
        frame_flags |= external_flags;

        self.dirty_nodes_this_frame
            .sort_unstable_by_key(|n| n.index);
        self.dirty_nodes_this_frame.dedup();

        let root_dirty = self.arena.get(self.root).state.core.dirty;
        let root_needs_layout =
            root_dirty.contains(DirtyFlags::LAYOUT) || root_dirty.contains(DirtyFlags::MEASURE);
        frame_flags |= root_dirty;

        let laid_out = self.execute_layout(
            window_constraints,
            root_needs_layout,
            frame_flags,
            layout_result,
        );
        let report_flags = frame_flags & !DirtyFlags::SUBTREE_DIRTY;

        FrameStats {
            flags: report_flags,
            nodes_dirtied: self.dirty_nodes_this_frame.len(),
            laid_out,
            drain_passes,
            paint_passes: 0,
        }
    }

    fn check_constraint_change(&mut self, window_constraints: Constraints) -> DirtyFlags {
        if self.prev_constraints != Some(window_constraints) {
            self.arena
                .mark_dirty(self.root, DirtyFlags::LAYOUT | DirtyFlags::MEASURE);
            self.dirty_nodes_this_frame.push(self.root);
            self.prev_constraints = Some(window_constraints);
            DirtyFlags::LAYOUT | DirtyFlags::MEASURE
        } else {
            DirtyFlags::NONE
        }
    }

    fn drain_reactive_mutations(&mut self) -> (DirtyFlags, usize) {
        const MAX_DRAIN_PASSES: usize = 16;
        let mut frame_flags = DirtyFlags::NONE;
        let mut passes = 0;

        loop {
            let dirty_subs = self.runtime.borrow_mut().drain_dirty_subscribers();
            if dirty_subs.is_empty() {
                break;
            }
            passes += 1;
            if passes > MAX_DRAIN_PASSES {
                // Requeue rather than drop: the batch was drained but not processed.
                self.runtime
                    .borrow_mut()
                    .requeue_dirty_subscribers(dirty_subs);
                break;
            }

            for sub in dirty_subs {
                if let Some((node, flags)) = self.router.lookup(sub) {
                    if self.arena.is_valid(node) {
                        if let Some(effective) = self.apply_node_binding(node, flags, sub) {
                            frame_flags.insert(effective);
                            self.dirty_nodes_this_frame.push(node);
                            self.arena.mark_dirty(node, effective);
                        }
                    }
                }
            }
        }
        (frame_flags, passes)
    }

    fn apply_node_binding(
        &mut self,
        node: NodeId,
        flags: DirtyFlags,
        sub: SubscriberId,
    ) -> Option<DirtyFlags> {
        let bindings = self.arena.get(node).state.core.bindings.clone();
        let mut effective = DirtyFlags::NONE;
        let mut pending_children: Option<(Vec<crate::dsl::element::Element>, Vec<Option<Key>>)> =
            None;
        {
            // The guard covers binding evaluation, so signal writes are attributed to the
            // subscriber that caused them rather than to reconciliation.
            let _guard = ObserverGuard::new(&self.runtime, Observer::Subscriber(sub));

            if flags.contains(DirtyFlags::MEASURE) || flags.contains(DirtyFlags::LAYOUT) {
                let n = self.arena.get_mut(node);
                let layout_changed =
                    bindings.update_layout(&mut n.style.width, &mut n.style.height);
                let text_changed = match n.kind {
                    NodeKind::Text(ref mut t) => {
                        let changed = bindings.update_text(&mut t.content);
                        if changed {
                            n.state.text.text_selection = None;
                        }
                        changed
                    }
                    _ => false,
                };
                let presence_changed = bindings.update_presence(&mut n.state.core.presence);

                if layout_changed || text_changed || presence_changed {
                    effective.insert(flags & (DirtyFlags::MEASURE | DirtyFlags::LAYOUT));
                }

                // A node that builds its children from reactive state re-evaluates them here, inside the
                // observer, so any signal the builder reads is tracked and will bring us
                // back. Comparing keys rather than element contents is what makes a two-row
                // swap cost one diff, and is why rows must be keyed by identity rather than
                // by position.
                if flags.contains(DirtyFlags::MEASURE) || flags.contains(DirtyFlags::LAYOUT) {
                    if let Some(build) = bindings.children_builder().cloned() {
                        // Entered here for the same reason as in `reconcile_group`: a list is
                        // rebuilt during a frame rather than at mount, so nothing has installed
                        // a hook scope for a `#[component]` child to allocate its signals on.
                        // Entering it is what lets a reactive list contain components at all.
                        let runtime = Rc::clone(&self.runtime);
                        let spec = aurora_reactive::Cx::new(runtime).enter(|| build());
                        let keys: Vec<Option<Key>> = spec.iter().map(|e| e.key()).collect();
                        pending_children = Some((spec, keys));
                    }
                }
            }

            // Bound values commit directly. There is no interception step that restores the
            // previous value and hands the delta to an engine-side controller, so a signal write
            // takes effect on the frame it happens and whatever reads that signal -- including a
            // child's own text -- moves with it.
            if flags.contains(DirtyFlags::PAINT) {
                let n = self.arena.get_mut(node);
                let tx_changed =
                    bindings.update_transform(n.state.core.declared_transform, &mut n.transform);
                let paint_changed = bindings.update_paint(
                    &mut n.style.appearance.fill,
                    &mut n.style.appearance.opacity,
                    &mut n.style.appearance.shadow,
                );
                // Stacking rides the paint path: it reorders paint without touching layout.
                let z_changed =
                    bindings.update_z(&mut n.style.z_index, n.style.is_absolute);
                if tx_changed || paint_changed || z_changed {
                    effective.insert(DirtyFlags::PAINT);
                }
            }
        }
        // Reconciliation writes styles and marks nodes, so it runs with no observer
        // installed rather than being attributed to the subscriber that triggered it.
        if let Some((spec, keys)) = pending_children {
            if keys != self.arena.get(node).state.core.child_keys {
                let runtime = Rc::clone(&self.runtime);
                let arena = &mut self.arena;
                let router = &mut self.router;
                crate::dsl::reconcile_children::reconcile_children(
                    &runtime, arena, router, node, spec,
                );
                arena.get_mut(node).state.core.child_keys = keys;
                // Reordering moves every sibling, so this subtree re-lays out and repaints
                // even though nothing about the node's own style changed.
                arena.mark_dirty(node, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
                effective.insert(DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
            }
        }

        if effective.is_empty() {
            None
        } else {
            Some(effective)
        }
    }

    /// Applies paint-class bindings that were written after layout.
    ///
    /// The reactive drain runs inside [`Self::commit_frame`], before layout, because a change to
    /// a node's style can change its size. That ordering leaves a gap: something that only
    /// becomes known *after* layout — a FLIP measuring where a row landed, a follow reading the
    /// box it was just given — writes a transform too late to be applied this frame, and the
    /// element it was written for is drawn one frame stale.
    ///
    /// Draining again here closes that gap, restricted to paint-class bindings. The
    /// restriction is what makes it safe to run after layout at all: a transform, a fill, an
    /// opacity or a shadow cannot change a box, so applying one cannot invalidate the layout that
    /// has already run. Anything that *could* change geometry is deliberately not reapplied —
    /// that would mean laying out twice per frame, and the second pass would need a third drain
    /// after it.
    ///
    /// Bounded like the main drain, for the same reason: a client that writes a bound signal from
    /// a post-layout callback would otherwise be able to spin here.
    pub fn drain_paint_after_layout(&mut self) -> (DirtyFlags, usize) {
        const MAX_DRAIN_PASSES: usize = 8;
        let mut frame_flags = DirtyFlags::NONE;
        let mut passes = 0;

        loop {
            let dirty_subs = self.runtime.borrow_mut().drain_dirty_subscribers();
            if dirty_subs.is_empty() {
                break;
            }
            passes += 1;
            if passes > MAX_DRAIN_PASSES {
                self.runtime
                    .borrow_mut()
                    .requeue_dirty_subscribers(dirty_subs);
                break;
            }
            let mut deferred = rustc_hash::FxHashSet::default();

            for sub in dirty_subs {
                // Anything that is not a pure paint subscriber is requeued for the next
                // frame's pre-layout drain, which can still act on it before layout runs.
                let Some((node, flags)) = self.router.lookup(sub) else {
                    continue;
                };
                if !flags.contains(DirtyFlags::PAINT) {
                    deferred.insert(sub);
                    continue;
                }
                // A subscriber that can also change geometry is left alone: reapplying it now
                // would dirty a box that layout has already placed, and the fix for that would
                // be a layout pass this frame cannot afford.
                if flags.contains(DirtyFlags::MEASURE) || flags.contains(DirtyFlags::LAYOUT) {
                    deferred.insert(sub);
                    continue;
                }
                if !self.arena.is_valid(node) {
                    continue;
                }
                if let Some(effective) = self.apply_node_binding(node, flags, sub) {
                    frame_flags.insert(effective);
                    self.dirty_nodes_this_frame.push(node);
                    self.arena.mark_dirty(node, effective);
                }
            }
            if !deferred.is_empty() {
                self.runtime
                    .borrow_mut()
                    .requeue_dirty_subscribers(deferred);
            }
        }
        (frame_flags, passes)
    }

    /// Drops marks recorded while the commit ran.
    ///
    /// `commit_frame` consumes the outbox first (external writes: input,
    /// engine), but the drain, reconciliation, and layout passes mark more
    /// nodes along the way — and the post-layout paint drain does too. Those
    /// are already represented: the drains push their nodes explicitly, and
    /// reconcile-time marks were invisible to the damage plan before the
    /// outbox existed. Carrying them into the *next* frame would report stale
    /// MEASURE/LAYOUT in its stats and could schedule spurious layout. Called
    /// once the post-layout drain has run, so only between-frame writes
    /// accumulate from here on.
    pub fn discard_commit_marks(&mut self) {
        self.arena.dirty_outbox.clear();
    }

    fn execute_layout(
        &mut self,
        window_constraints: Constraints,
        root_needs_layout: bool,
        frame_flags: DirtyFlags,
        layout_result: &mut LayoutResult,
    ) -> bool {
        const MAX_INDEPENDENT_SUBTREES: usize = 16;
        if root_needs_layout {
            layout_node_with_text(
                &mut self.arena,
                self.root,
                window_constraints,
                &self.text_ctx,
                layout_result,
                Point::ZERO,
            );
            return true;
        }
        if !frame_flags.contains(DirtyFlags::LAYOUT) && !frame_flags.contains(DirtyFlags::MEASURE) {
            return false;
        }

        let mut boundaries =
            collect_layout_boundaries(&self.arena, self.root, &self.dirty_nodes_this_frame);
        if boundaries.len() > MAX_INDEPENDENT_SUBTREES || boundaries.contains(&self.root) {
            layout_node_with_text(
                &mut self.arena,
                self.root,
                window_constraints,
                &self.text_ctx,
                layout_result,
                Point::ZERO,
            );
            return true;
        }

        prune_nested_boundaries(&self.arena, &mut boundaries);
        for boundary in boundaries {
            if !self.arena.is_valid(boundary) {
                continue;
            }
            let node = self.arena.get(boundary);
            if node.is_layout_boundary() {
                let constraints = node.state.core.cache.last_constraints.unwrap_or_else(|| {
                    let (w, h) = match (node.style.width, node.style.height) {
                        (aurora_foundation::Size::Fixed(w), aurora_foundation::Size::Fixed(h)) => {
                            (w, h)
                        }
                        _ => (0.0, 0.0),
                    };
                    Constraints::tight(w, h)
                });

                // Compute boundary parent absolute origin once before descending top-down
                let parent_abs = self
                    .arena
                    .parent(boundary)
                    .map_or(Point::ZERO, |p| self.arena.node_absolute_point(p));

                layout_node_with_text(
                    &mut self.arena,
                    boundary,
                    constraints,
                    &self.text_ctx,
                    layout_result,
                    parent_abs,
                );
            } else {
                layout_node_with_text(
                    &mut self.arena,
                    self.root,
                    window_constraints,
                    &self.text_ctx,
                    layout_result,
                    Point::ZERO,
                );
                break;
            }
        }
        true
    }
}
