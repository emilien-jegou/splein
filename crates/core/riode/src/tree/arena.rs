// Single responsibility: Generational arena managing node allocation, hierarchy, and invalidation.

use crate::foundation::{
    GlassPress, Key, MeasuredElement, Point, ResolvedRect, Transform, GLASS_STANDARD,
};
use crate::tree::flags::DirtyFlags;
use crate::tree::id::NodeId;
use crate::tree::invalidation::mark_node_dirty;
use crate::tree::kind::NodeKind;
use crate::tree::node::LayoutNode;
use crate::tree::registry::KeyRegistry;
use smallvec::SmallVec;

struct Slot {
    node: LayoutNode,
    generation: u32,
    is_active: bool,
    parent: Option<NodeId>,
    /// First child in document order [S2-3 CSR]: siblings chain through
    /// `next_sibling`. No per-parent heap allocation (was `Vec<NodeId>`).
    first_child: Option<NodeId>,
    next_sibling: Option<NodeId>,
}

/// Borrowed child iterator over CSR sibling links: `Copy` ids, no allocation.
/// Same borrow rules as the old child slice (immutable arena borrow for the
/// loop). For reversed or indexed access use `children_vec`.
#[derive(Clone)]
pub struct ChildIter<'a> {
    arena: &'a TreeArena,
    next: Option<NodeId>,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        let cur = self.next?;
        self.next = self.arena.slots[cur.index as usize].next_sibling;
        Some(cur)
    }
}

/// Generational arena managing retained layout node lifecycles.
pub struct TreeArena {
    slots: Vec<Slot>,
    free_indices: Vec<u32>,
    /// On-screen bounding boxes of nodes unmounted since the last paint pass.
    pub removed_damage: Vec<ResolvedRect>,
    /// Stable key index used to reach retained nodes across frames.
    keys: KeyRegistry,
    /// Every node marked dirty since the last frame commit, with its flags.
    ///
    /// Input and engine writes mark arena flags directly, bypassing the
    /// reactive drains — without this outbox their nodes never reached
    /// `dirty_nodes_this_frame`, so the damage plan walked an empty list on
    /// input-only frames and paint-only highlights never rasterized (state
    /// set, scene recompiled, pixels unchanged). `commit_frame` drains this
    /// alongside the reactive mutations and dedups. Entries for nodes
    /// removed mid-commit are skipped by validity checks downstream.
    pub dirty_outbox: SmallVec<[(NodeId, DirtyFlags); 8]>,
    /// Frame stamp for `LayoutNode::abs_tx` caching [STAGE1 #15].
    /// See `bump_tx_frame` for the invalidation discipline.
    pub tx_frame: u64,
}

impl TreeArena {
    /// Creates a new empty tree arena.
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_indices: Vec::new(),
            removed_damage: Vec::new(),
            keys: KeyRegistry::new(),
            dirty_outbox: SmallVec::new(),
            tx_frame: 1,
        }
    }

    /// Inserts a layout node and returns its generational identifier.
    pub fn insert(&mut self, node: LayoutNode) -> NodeId {
        if let Some(idx) = self.free_indices.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.node = node;
            slot.is_active = true;
            slot.parent = None;
            slot.first_child = None;
            slot.next_sibling = None;
            NodeId::new(idx, slot.generation)
        } else {
            let idx = self.slots.len() as u32;
            self.slots.push(Slot {
                node,
                generation: 1,
                is_active: true,
                parent: None,
                first_child: None,
                next_sibling: None,
            });
            NodeId::new(idx, 1)
        }
    }

    /// Removes a node and all recursive children from the arena, invoking a visitor on each.
    ///
    /// The top node is unlinked from its parent once; descendants die with
    /// it and never pay a sibling scan of their own. Children are taken
    /// (not cloned) and the subtree drains over an explicit stack, so deep
    /// trees cannot overflow the call stack and teardown allocates nothing.
    ///
    /// The visitor also receives the node's reconciliation key: a keyed row's hook state is
    /// filed under an instance name the node never carries, so removal — which is only
    /// observable here, by node — needs the key to look the instance back up.
    pub fn remove_with<F: FnMut(NodeId, Option<Key>)>(&mut self, id: NodeId, mut on_remove: F) {
        if !self.is_valid(id) {
            return;
        }
        // Capture the surviving parent up front: the loop below kills slots
        // (and their generations), so re-deriving it afterwards is unsound.
        let live_parent = self.slots[id.index as usize]
            .parent
            .filter(|&p| self.is_valid(p));
        // Unlink the top node from its sibling chain (order-preserving).
        if let Some(parent) = live_parent {
            let mut prev: Option<NodeId> = None;
            let mut cur = self.slots[parent.index as usize].first_child;
            while let Some(c) = cur {
                let next = self.slots[c.index as usize].next_sibling;
                if c == id {
                    if let Some(p) = prev {
                        self.slots[p.index as usize].next_sibling = next;
                    } else {
                        self.slots[parent.index as usize].first_child = next;
                    }
                    break;
                }
                prev = Some(c);
                cur = next;
            }
        }
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if !self.is_valid(cur) {
                continue;
            }
            // Drain children through the sibling chain (no allocation beyond
            // the explicit stack): unlink each child as it is pushed.
            let mut child = self.slots[cur.index as usize].first_child;
            self.slots[cur.index as usize].first_child = None;
            while let Some(c) = child {
                child = self.slots[c.index as usize].next_sibling;
                self.slots[c.index as usize].next_sibling = None;
                stack.push(c);
            }
            let key = self.slots[cur.index as usize].node.key.clone();
            on_remove(cur, key);
            self.keys.unbind(cur);

            if let Some(bounds) = self.slots[cur.index as usize]
                .node
                .state
                .paint
                .last_painted_bounds
            {
                self.removed_damage.push(bounds);
            }
            let slot = &mut self.slots[cur.index as usize];
            slot.is_active = false;
            slot.generation += 1;
            slot.parent = None;
            slot.first_child = None;
            slot.next_sibling = None;
            // Children were unlinked above; nothing left to clear.
            self.free_indices.push(cur.index);
        }
        // The surviving parent's child list changed: its static proof is void.
        if let Some(parent) = live_parent {
            if self.is_valid(parent) {
                self.invalidate_static_up(parent);
            }
        }
    }

    /// Drains all accumulated damage from unmounted nodes.
    #[inline(always)]
    pub fn drain_removed_damage(&mut self) -> Vec<ResolvedRect> {
        std::mem::take(&mut self.removed_damage)
    }

    /// Live liquid-glass lenses for damage repair: committed bounds plus the
    /// backdrop-input reach in screen px.
    ///
    /// A lens refracts backdrop from outside its own rect (up to its damage
    /// pad away), so backdrop motion inside that reach must repaint the lens
    /// even though the lens node itself is static and contributes no damage
    /// of its own. Pure read over committed state (same pattern as
    /// [`Self::unsettled_text_bounds`]); the damage pass calls this after
    /// its walk and repaints the stale slivers (see `repair_glass_lenses`).
    ///
    /// The reach rescales the layout-px pad by the node's own scale, derived
    /// from committed vs layout size and biased upward (clamped ≥ 1.0):
    /// overestimating only overpaints, underestimating would miss pixels.
    pub fn glass_lens_bounds(&self) -> SmallVec<[(ResolvedRect, f32); 4]> {
        let mut out = SmallVec::new();
        for slot in &self.slots {
            if !slot.is_active {
                continue;
            }
            let Some((glass, backdrop, _, _, _)) = slot
                .node
                .style
                .appearance
                .backdrop_effects
                .as_ref()
                .and_then(|s| s.glass_single())
            else {
                continue;
            };
            let Some(bounds) = slot.node.state.paint.last_painted_bounds else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }
            let pad = glass.damage_pad(&backdrop);
            let (lw, lh) = (
                slot.node.resolved_rect.width + 2.0 * pad,
                slot.node.resolved_rect.height + 2.0 * pad,
            );
            let sx = if lw > 0.0 { bounds.width / lw } else { 1.0 };
            let sy = if lh > 0.0 { bounds.height / lh } else { 1.0 };
            let scale = sx.max(sy).max(1.0);
            // Non-finite scale (degenerate layout; never observed since
            // committed bounds derive from the same layout size) falls back
            // to unscaled pad: overpaints rather than missing pixels.
            let reach = if scale.is_finite() {
                pad * scale
            } else {
                pad
            };
            out.push((bounds, reach));
        }
        out
    }
    /// Whether any glass group still has morph tracks gliding (drives
    /// `has_active_motion` so the frame loop keeps running until every
    /// glide lands).
    pub fn morph_unsettled(&self) -> bool {
        self.slots.iter().any(|slot| {
            slot.is_active
                && slot
                    .node
                    .style
                    .appearance
                    .backdrop_effects
                    .as_ref()
                    .is_some_and(|s| s.glass_group_spacing().is_some())
                && slot.node.state.glass_morph.is_active()
        })
    }

    /// Whether any glass motion still wants frames: morph glides or an
    /// active press (finger down, release spring or light hold settling).
    pub fn glass_motion_active(&self) -> bool {
        self.morph_unsettled() || self.press_active()
    }

    /// Whether any glass node holds an active press.
    pub fn press_active(&self) -> bool {
        self.slots.iter().any(|slot| {
            slot.is_active && slot.node.state.glass_press.as_ref().is_some_and(|p| p.is_active())
        })
    }

    /// Starts a press on `id` at window-space `pos` (or retargets the
    /// same node's press). Any other node's press ends first: one active
    /// press at a time (primary pointer).
    pub fn press_begin(&mut self, id: NodeId, pos: Point) {
        let motion = GLASS_STANDARD.motion;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if !slot.is_active {
                continue;
            }
            if slot.node.state.glass_press.is_some()
                && NodeId::new(i as u32, slot.generation) != id
            {
                if let Some(press) = slot.node.state.glass_press.as_mut() {
                    press.up(&motion);
                }
            }
        }
        if !self.is_valid(id) {
            return;
        }
        let node = self.get(id);
        let origin = Point::new(node.absolute_rect.x, node.absolute_rect.y);
        let touch = Point::new(pos.x - origin.x, pos.y - origin.y);
        let (w, h) = (node.resolved_rect.width, node.resolved_rect.height);
        let node = self.get_mut(id);
        match node.state.glass_press.as_mut() {
            Some(press) => press.retarget_down(touch, w, h, &motion),
            None => {
                node.state.glass_press = Some(GlassPress::down(touch, w, h, &motion));
            }
        }
        // Press state integrates in the damage pass and never marks
        // dirty on its own: flag the subtree so this frame recompiles
        // instead of replaying the stale stream (steady-state frames
        // ride the chunk's `glass_unsettled` flag instead).
        self.mark_dirty(id, DirtyFlags::PAINT);
    }

    /// Moves the active press's touch to window-space `pos`.
    pub fn press_move(&mut self, pos: Point) {
        for slot in &mut self.slots {
            if !slot.is_active {
                continue;
            }
            if let Some(press) = slot.node.state.glass_press.as_mut() {
                if press.is_active() {
                    let origin = Point::new(
                        slot.node.absolute_rect.x,
                        slot.node.absolute_rect.y,
                    );
                    press.move_to(Point::new(pos.x - origin.x, pos.y - origin.y));
                    return;
                }
            }
        }
    }

    /// Releases the active press (release spring + light hold take over).
    pub fn press_end(&mut self) {
        let motion = GLASS_STANDARD.motion;
        for slot in &mut self.slots {
            if !slot.is_active {
                continue;
            }
            if let Some(press) = slot.node.state.glass_press.as_mut() {
                press.up(&motion);
            }
        }
    }

    /// Advances every press one fixed quantum. Returns absolute motion
    /// bounds (current ∪ target applied shapes) for damage; empty when
    /// nothing moved.
    pub fn advance_glass_press(&mut self) -> Vec<ResolvedRect> {
        let motion = GLASS_STANDARD.motion;
        let mut out = Vec::new();
        for slot in &mut self.slots {
            if !slot.is_active {
                continue;
            }
            let origin = Point::new(slot.node.absolute_rect.x, slot.node.absolute_rect.y);
            let (w, h) = (slot.node.resolved_rect.width, slot.node.resolved_rect.height);
            let Some(press) = slot.node.state.glass_press.as_mut() else {
                continue;
            };
            if !press.is_active() {
                continue;
            }
            let base = ResolvedRect::new(origin.x, origin.y, w, h);
            // Damage covers the last advance's touch as well as this
            // one: fast finger travel deforms faster than the spring.
            let before = press.geometry_for(&motion, press.last_touch()).apply(base);
            press.advance();
            let after = press.geometry(&motion).apply(base);
            let moved = before.union(&after).expand(2.0);
            out.push(moved);
            // A press that fully settled frees its state (a fresh down
            // starts clean; `is_active` already covers the light hold).
            if !press.is_active() {
                slot.node.state.glass_press = None;
            }
        }
        out
    }

    /// Advances every glass-group morph track one fixed quantum.
    /// Returns group-local motion bounds with the group's absolute
    /// origin for damage (empty when nothing moved).
    pub fn advance_glass_morphs(&mut self) -> Vec<(ResolvedRect, f32, f32)> {
        let mut out = Vec::new();
        for slot in &mut self.slots {
            let is_group = slot
                .node
                .style
                .appearance
                .backdrop_effects
                .as_ref()
                .is_some_and(|s| s.glass_group_spacing().is_some());
            if !slot.is_active || !is_group {
                continue;
            }
            let (moved, _) = slot.node.state.glass_morph.advance();
            if let Some(local) = moved {
                out.push((
                    local,
                    slot.node.absolute_rect.x,
                    slot.node.absolute_rect.y,
                ));
            }
        }
        out
    }
    /// Idle backstop for text settling: collects repaint regions for text
    /// runs that still need observation to reach their baseline.
    ///
    /// Pure read, no state change: observation itself only runs during
    /// compile (where absolute positions are live). On otherwise-idle
    /// frames this reports the runs mid-glide so the frame forces a
    /// recompile instead of freezing them mid-nudge. Bounded: a glide
    /// converges in a frame or two, then this reports nothing and idle
    /// stays idle. Regions are the last painted bounds expanded by the
    /// max single-frame nudge, covering old and new pixels.
    pub fn unsettled_text_bounds(&self) -> SmallVec<[ResolvedRect; 4]> {
        let mut out = SmallVec::new();
        for slot in &self.slots {
            if !slot.is_active {
                continue;
            }
            if !matches!(slot.node.kind, NodeKind::Text(_)) {
                continue;
            }
            if !slot.node.state.text.placement.needs_settle() {
                continue;
            }
            if let Some(b) = slot.node.state.paint.last_painted_bounds {
                out.push(b.expand(2.0));
            }
        }
        out
    }

    /// Checks if a node handle refers to an active, non-stale arena slot.
    #[inline(always)]
    pub fn is_valid(&self, id: NodeId) -> bool {
        self.slots
            .get(id.index as usize)
            .is_some_and(|s| s.is_active && s.generation == id.generation)
    }

    /// Immutably accesses a validated layout node.
    #[inline(always)]
    pub fn get(&self, id: NodeId) -> &LayoutNode {
        assert!(self.is_valid(id), "Accessing dangling NodeId");
        &self.slots[id.index as usize].node
    }

    /// Mutably accesses a validated layout node.
    #[inline(always)]
    pub fn get_mut(&mut self, id: NodeId) -> &mut LayoutNode {
        assert!(self.is_valid(id), "Mutating dangling NodeId");
        &mut self.slots[id.index as usize].node
    }

    /// Marks a node dirty and propagates invalidation up to containing layout boundaries.
    ///
    /// Epoch discipline (not walks): every mark bumps `tx_frame`, invalidating
    /// all cached absolute transforms in O(1). A per-mark subtree walk would
    /// cost O(K) plus an allocator hit on the hottest path in the engine
    /// (animated text marks per leaf per frame); the memo re-fills during the
    /// damage/compile passes that follow, where queries cluster and nothing
    /// mutates geometry anymore. Paint-only marks bump too — one increment
    /// is cheaper than proving paint didn't move anything, and a stale cache
    /// is never worth the case analysis.
    pub fn mark_dirty(&mut self, node_id: NodeId, flags: DirtyFlags) {
        self.bump_tx_frame();
        mark_node_dirty(self, node_id, flags);
    }

    /// Advances the absolute-transform cache epoch, invalidating every cached
    /// entry at once. Call after any geometry mutation (marks, placements,
    /// reparenting): O(1), always safe, occasionally conservative. Never
    /// worse than no cache — queries simply re-walk once per chain per epoch.
    pub fn bump_tx_frame(&mut self) {
        self.tx_frame = self.tx_frame.wrapping_add(1).max(1);
    }

    /// Records a node's reconciliation key and indexes it for stable lookup.
    pub fn bind_key(&mut self, key: Option<Key>, node: NodeId) {
        self.get_mut(node).key = key.clone();
        match key {
            Some(key) => self.keys.bind(key, node),
            None => self.keys.unbind(node),
        }
    }

    /// Publishes each watched node's geometry, plus its children's, into the frame's table.
    pub fn publish_watched(&self, watched: &[NodeId], mut put: impl FnMut(u32, MeasuredElement)) {
        for &node in watched {
            if !self.is_valid(node) {
                // A purged node leaves a stale handle behind. Skipping it is what lets a handle
                // outlive the element it was bound to without ever reading a dead slot.
                continue;
            }
            // Raw layout boxes, not visual bounds: handles are a measurement path
            // (slot tables match on exact heights, FLIP compares deltas), and padding
            // would corrupt both. Paint coverage keeps its own pad in the damage and
            // compile passes, which call `compute_visual_bounds` directly.
            let children: Box<[ResolvedRect]> = self
                .children(node)
                .iter()
                .filter(|&&c| self.is_valid(c))
                .map(|&c| self.get(c).absolute_rect)
                .collect();
            put(
                node.index,
                MeasuredElement {
                    rect: self.get(node).absolute_rect,
                    children,
                },
            );
        }
    }

    /// Resolves a stable key to a retained node that is still alive.
    /// Resolves a stable key to a retained node that is still alive.
    pub fn node_for_key(&self, key: &Key) -> Option<NodeId> {
        let id = self.keys.resolve(key)?;
        self.is_valid(id).then_some(id)
    }

    /// Clears the static-subtree memo on a node and all its ancestors [S2-2].
    ///
    /// Call when the node's bindings, providers, or child structure change:
    /// every ancestor's proof depends on this subtree. Descendants keep
    /// their proofs (their interiors didn't change). Deliberately NOT called
    /// from `mark_dirty` — dirty means "changed", not "dynamic".
    pub fn invalidate_static_up(&mut self, node_id: NodeId) {
        let mut curr = Some(node_id);
        while let Some(n) = curr {
            if !self.is_valid(n) {
                break;
            }
            self.slots[n.index as usize].node.state.core.subtree_static = None;
            curr = self.slots[n.index as usize].parent;
        }
    }

    /// Attaches child nodes to a parent container.
    pub fn set_children(&mut self, parent: NodeId, new_children: Vec<NodeId>) {
        assert!(self.is_valid(parent));
        let mut first: Option<NodeId> = None;
        let mut prev: Option<NodeId> = None;
        for &child in &new_children {
            assert!(self.is_valid(child));
            self.slots[child.index as usize].parent = Some(parent);
            self.slots[child.index as usize].next_sibling = None;
            if let Some(p) = prev {
                self.slots[p.index as usize].next_sibling = Some(child);
            } else {
                first = Some(child);
            }
            prev = Some(child);
        }
        self.slots[parent.index as usize].first_child = first;
        // Structure changed: ancestors' static proofs (which quantify over
        // children) are void. The children's own proofs stay valid.
        self.invalidate_static_up(parent);
        // Reparenting rewrites absolute chains below: bump the epoch (O(1)).
        self.bump_tx_frame();
    }

    /// Iterates immediate child node IDs in document order (no allocation).
    #[inline(always)]
    pub fn each_child(&self, id: NodeId) -> ChildIter<'_> {
        let next = if self.is_valid(id) {
            self.slots[id.index as usize].first_child
        } else {
            None
        };
        ChildIter { arena: self, next }
    }

    /// Collects child IDs into a stack-resident buffer (spills to heap past
    /// 8 children). For indexed loops and borrow-conflicting bodies that the
    /// borrowed [`Self::each_child`] iterator cannot serve.
    #[inline(always)]
    pub fn children_vec(&self, id: NodeId) -> SmallVec<[NodeId; 8]> {
        self.each_child(id).collect()
    }

    /// Returns the immediate child node IDs of a parent container.
    ///
    /// Compatibility shim over [`Self::children_vec`]: prefer `each_child`
    /// (borrowed iteration) or `children_vec` (owned) at new call sites.
    #[inline(always)]
    pub fn children(&self, id: NodeId) -> SmallVec<[NodeId; 8]> {
        self.children_vec(id)
    }

    /// Returns the parent node ID of a child if attached.
    #[inline(always)]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        if !self.is_valid(id) {
            return None;
        }
        self.slots[id.index as usize].parent
    }

    /// Computes the absolute screen-space transformation matrix for a node.
    /// Uses a per-frame memo on each node [STAGE1 #15]: the walk stops at
    /// the nearest ancestor whose `abs_tx` is current, then fills the cache
    /// on the way back down, so M queries over depth D cost O(M+D).
    pub fn node_absolute_transform(&self, id: NodeId) -> Transform {
        if !self.is_valid(id) {
            return Transform::IDENTITY;
        }
        if self.get(id).abs_tx_frame == self.tx_frame {
            return self.get(id).abs_tx;
        }
        let mut path = SmallVec::<[NodeId; 16]>::new();
        let mut curr = Some(id);
        while let Some(n) = curr {
            if self.get(n).abs_tx_frame == self.tx_frame {
                break;
            }
            path.push(n);
            curr = self.parent(n);
        }
        let mut tx = curr
            .map(|n| self.get(n).abs_tx)
            .unwrap_or(Transform::IDENTITY);
        // path is leaf-up; iterate reversed to go top-down. Cache writes need
        // `&mut`, so collect first then write via raw index (ids validated).
        let order: SmallVec<[NodeId; 16]> = path.iter().rev().copied().collect();
        let frame = self.tx_frame;
        let arena_ptr = self as *const Self as *mut Self;
        for &node_id in &order {
            // SAFETY: ids were validated above; no insert/remove happens here.
            let node = unsafe { (*arena_ptr).get(node_id) };
            tx = tx.multiply(&Transform::from_translation(
                node.resolved_rect.x,
                node.resolved_rect.y,
            ));
            let local_tx = node.effective_transform();
            if local_tx != Transform::IDENTITY {
                tx = tx.multiply(&local_tx);
            }
            let slot = unsafe { (*arena_ptr).get_mut(node_id) };
            slot.abs_tx = tx;
            slot.abs_tx_frame = frame;
        }
        tx
    }

    /// Computes the absolute screen-space origin point of a node.
    pub fn node_absolute_point(&self, id: NodeId) -> Point {
        if !self.is_valid(id) {
            return Point::ZERO;
        }
        let tx = self.node_absolute_transform(id);
        tx.transform_point(Point::new(0.0, 0.0))
    }

    /// Computes the absolute screen-space bounding box of a node.
    ///
    /// The box is projected, not rebuilt from its origin and its local size. An earlier version
    /// took the absolute origin point and paired it with the node's untransformed width and
    /// height, which is only correct while the accumulated transform is a pure translation: under
    /// rotation or scale it reported a box in the wrong place with the wrong extent. Sizing from
    /// the same transform the paint pass uses keeps this the axis-aligned envelope of what is
    /// actually drawn, which is what a caller asking for absolute bounds expects.
    pub fn node_absolute_rect(&self, id: NodeId) -> ResolvedRect {
        if !self.is_valid(id) {
            return ResolvedRect::ZERO;
        }
        let tx = self.node_absolute_transform(id);
        self.get(id).compute_visual_bounds(&tx, None)
    }

    /// Tight layout box in absolute window coordinates, projected through the live transform
    /// chain but without AA pad, stroke, or shadow.
    ///
    /// The measurement counterpart to [`node_absolute_rect`](Self::node_absolute_rect):
    /// settled, it equals the stored layout box the snapshot publishes; in flight, it tracks
    /// the transform. Node queries use this so handles and the engine agree.
    pub fn node_absolute_layout_rect(&self, id: NodeId) -> ResolvedRect {
        if !self.is_valid(id) {
            return ResolvedRect::ZERO;
        }
        let tx = self.node_absolute_transform(id);
        self.get(id).compute_layout_bounds(&tx)
    }
}

impl Default for TreeArena {
    fn default() -> Self {
        Self::new()
    }
}
