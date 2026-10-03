// Single responsibility: Multi-phase frame execution, metrics reporting, and reactive queue drainage.

use smallvec::SmallVec;
use std::cell::RefCell;
use std::rc::Rc;

use crate::foundation::{Constraints, Point, ResolvedRect};
use crate::layout::{layout_node_with_text, LayoutResult};
use crate::motion::vector::MotionVector;
use crate::motion::MotionRuntime;
use crate::motion::MotionState;
use crate::reactive::{Observer, ReactiveRuntime, SubscriberId};
use crate::runtime::boundary::{collect_layout_boundaries, prune_nested_boundaries};
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, NodeKind, SubscriberRouter, TreeArena};

/// Execution summary of reactive updates and layout recalculations.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub flags: DirtyFlags,
    pub nodes_dirtied: usize,
    pub laid_out: bool,
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
    /// Node updates applied outside reactive drainage, awaiting the next frame pass.
    pub pending_updates: SmallVec<[(NodeId, DirtyFlags); 4]>,
    /// Clock and controllers driving compositor motion.
    pub motion: MotionRuntime,
    /// Child rects captured before the current layout pass, for FLIP deltas.
    pub flip_snapshot: SmallVec<[(NodeId, NodeId, ResolvedRect); 16]>,
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
            pending_updates: SmallVec::new(),
            motion: MotionRuntime::new(),
            flip_snapshot: SmallVec::new(),
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
        for (node, flags) in std::mem::take(&mut self.pending_updates) {
            self.dirty_nodes_this_frame.push(node);
            external_flags |= flags;
        }

        let mut frame_flags = self.check_constraint_change(window_constraints);
        frame_flags |= self.drain_reactive_mutations();
        frame_flags |= external_flags;

        self.dirty_nodes_this_frame
            .sort_unstable_by_key(|n| n.index);
        self.dirty_nodes_this_frame.dedup();

        let root_dirty = self.arena.get(self.root).state.dirty;
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

    fn drain_reactive_mutations(&mut self) -> DirtyFlags {
        const MAX_DRAIN_PASSES: usize = 16;
        let mut frame_flags = DirtyFlags::NONE;
        let mut passes = 0;

        loop {
            let dirty_subs = self.runtime.borrow_mut().drain_dirty_subscribers();
            if dirty_subs.is_empty() {
                break;
            }
            passes += 1;
            if passes >= MAX_DRAIN_PASSES {
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
        frame_flags
    }

    fn apply_node_binding(
        &mut self,
        node: NodeId,
        flags: DirtyFlags,
        sub: SubscriberId,
    ) -> Option<DirtyFlags> {
        let bindings = self.arena.get(node).state.bindings.clone();
        let mut effective = DirtyFlags::NONE;
        let redirected = {
            // The guard covers only binding evaluation, so motion can retarget afterwards.
            let _guard = ObserverGuard::new(&self.runtime, Observer::Subscriber(sub));

            if flags.contains(DirtyFlags::MEASURE) || flags.contains(DirtyFlags::LAYOUT) {
                let n = self.arena.get_mut(node);
                let layout_changed = bindings.update_layout(&mut n.style.width, &mut n.style.height);
                let text_changed = match n.kind {
                    NodeKind::Text(ref mut t) => bindings.update_text(&mut t.content),
                    _ => false,
                };
                let presence_changed = bindings.update_presence(&mut n.state.presence);

                if layout_changed || text_changed || presence_changed {
                    effective.insert(flags & (DirtyFlags::MEASURE | DirtyFlags::LAYOUT));
                }
            }

            let mut redirect = None;
            if flags.contains(DirtyFlags::PAINT) {
                let snapshot = {
                    let n = self.arena.get(node);
                    MotionVector::from_state(&MotionState {
                        transform: n.transform,
                        opacity: n.style.appearance.opacity,
                    })
                };
                let n = self.arena.get_mut(node);
                let tx_changed = bindings.update_transform(&mut n.transform);
                let paint_changed = bindings.update_paint(
                    &mut n.style.appearance.fill,
                    &mut n.style.appearance.opacity,
                    &mut n.style.appearance.shadows,
                );
                let opacity_changed = n.style.appearance.opacity != snapshot.opacity;

                if tx_changed || opacity_changed {
                    redirect = Some((snapshot, tx_changed, opacity_changed));
                }
                if tx_changed || paint_changed {
                    effective.insert(DirtyFlags::PAINT);
                }
            }
            redirect
        };

        if let Some((snapshot, tx_changed, opacity_changed)) = redirected {
            self.redirect_change(node, snapshot, tx_changed, opacity_changed);
        }

        if effective.is_empty() {
            None
        } else {
            Some(effective)
        }
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
                let constraints = node.state.cache.last_constraints.unwrap_or_else(|| {
                    let (w, h) = match (node.style.width, node.style.height) {
                        (crate::foundation::Size::Fixed(w), crate::foundation::Size::Fixed(h)) => {
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
