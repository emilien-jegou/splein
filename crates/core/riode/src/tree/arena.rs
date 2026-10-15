// Single responsibility: Generational arena managing node allocation, hierarchy, and invalidation.

use crate::foundation::{Key, MeasuredElement, Point, ResolvedRect, Transform};
use crate::tree::flags::DirtyFlags;
use crate::tree::id::NodeId;
use crate::tree::invalidation::mark_node_dirty;
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
