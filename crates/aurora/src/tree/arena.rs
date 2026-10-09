// Single responsibility: Generational arena managing node allocation, hierarchy, and invalidation.

use aurora_foundation::{Key, MeasuredElement, Point, ResolvedRect, Transform};
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
    children: Vec<NodeId>,
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
        }
    }

    /// Inserts a layout node and returns its generational identifier.
    pub fn insert(&mut self, node: LayoutNode) -> NodeId {
        if let Some(idx) = self.free_indices.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.node = node;
            slot.is_active = true;
            slot.parent = None;
            slot.children.clear();
            NodeId::new(idx, slot.generation)
        } else {
            let idx = self.slots.len() as u32;
            self.slots.push(Slot {
                node,
                generation: 1,
                is_active: true,
                parent: None,
                children: Vec::new(),
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
    pub fn remove_with<F: FnMut(NodeId)>(&mut self, id: NodeId, mut on_remove: F) {
        if !self.is_valid(id) {
            return;
        }
        if let Some(parent) = self.slots[id.index as usize].parent {
            if self.is_valid(parent) {
                // Single order-preserving unlink for the top node only:
                // one shift here replaces a scan per descendant, and
                // surviving siblings keep document order.
                let siblings = &mut self.slots[parent.index as usize].children;
                if let Some(pos) = siblings.iter().position(|&c| c == id) {
                    siblings.remove(pos);
                }
            }
        }
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if !self.is_valid(cur) {
                continue;
            }
            let children = std::mem::take(&mut self.slots[cur.index as usize].children);
            stack.extend(children.into_iter());
            on_remove(cur);
            self.keys.unbind(cur);

            if let Some(bounds) = self.slots[cur.index as usize].node.state.last_painted_bounds {
                self.removed_damage.push(bounds);
            }
            let slot = &mut self.slots[cur.index as usize];
            slot.is_active = false;
            slot.generation += 1;
            slot.parent = None;
            // Children were taken above, so the buffer is already empty and
            // keeps its capacity for the slot's next occupant.
            self.free_indices.push(cur.index);
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
    pub fn mark_dirty(&mut self, node_id: NodeId, flags: DirtyFlags) {
        mark_node_dirty(self, node_id, flags);
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

    /// Attaches child nodes to a parent container.
    pub fn set_children(&mut self, parent: NodeId, new_children: Vec<NodeId>) {
        assert!(self.is_valid(parent));
        for &child in &new_children {
            assert!(self.is_valid(child));
            self.slots[child.index as usize].parent = Some(parent);
        }
        self.slots[parent.index as usize].children = new_children;
    }

    /// Returns the immediate child node IDs of a parent container.
    #[inline(always)]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        if !self.is_valid(id) {
            return &[];
        }
        &self.slots[id.index as usize].children
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
    pub fn node_absolute_transform(&self, id: NodeId) -> Transform {
        if !self.is_valid(id) {
            return Transform::IDENTITY;
        }
        let mut path = SmallVec::<[NodeId; 16]>::new();
        let mut curr = Some(id);
        while let Some(n) = curr {
            path.push(n);
            curr = self.parent(n);
        }
        path.reverse();

        let mut tx = Transform::IDENTITY;
        for &node_id in &path {
            let node = self.get(node_id);
            tx = tx.multiply(&Transform::from_translation(
                node.resolved_rect.x,
                node.resolved_rect.y,
            ));
            let local_tx = node.effective_transform();
            if local_tx != Transform::IDENTITY {
                tx = tx.multiply(&local_tx);
            }
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
}

impl Default for TreeArena {
    fn default() -> Self {
        Self::new()
    }
}
