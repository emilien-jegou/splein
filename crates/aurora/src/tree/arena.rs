// Single responsibility: Generational arena managing node allocation, hierarchy, and invalidation.

use crate::foundation::{Key, Point, ResolvedRect, Transform};
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
    /// Nodes whose children animate toward new layout rects.
    layout_transitions: Vec<NodeId>,
}

impl TreeArena {
    /// Creates a new empty tree arena.
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_indices: Vec::new(),
            removed_damage: Vec::new(),
            keys: KeyRegistry::new(),
            layout_transitions: Vec::new(),
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
    pub fn remove_with<F: FnMut(NodeId)>(&mut self, id: NodeId, mut on_remove: F) {
        self.remove_internal(id, &mut on_remove);
    }

    fn remove_internal<F: FnMut(NodeId)>(&mut self, id: NodeId, on_remove: &mut F) {
        if !self.is_valid(id) {
            return;
        }
        if let Some(parent) = self.slots[id.index as usize].parent {
            if self.is_valid(parent) {
                self.slots[parent.index as usize]
                    .children
                    .retain(|&c| c != id);
            }
        }
        for child in self.slots[id.index as usize].children.clone() {
            self.remove_internal(child, on_remove);
        }
        on_remove(id);
        self.keys.unbind(id);
        self.layout_transitions.retain(|&other| other != id);

        let slot = &mut self.slots[id.index as usize];
        if let Some(bounds) = slot.node.state.last_painted_bounds {
            self.removed_damage.push(bounds);
        }
        slot.is_active = false;
        slot.generation += 1;
        slot.parent = None;
        slot.children.clear();
        self.free_indices.push(id.index);
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

    /// Resolves a stable key to a retained node that is still alive.
    pub fn node_for_key(&self, key: &Key) -> Option<NodeId> {
        let id = self.keys.resolve(key)?;
        self.is_valid(id).then_some(id)
    }

    /// Records a node whose children animate toward new layout rects.
    pub fn note_layout_transition(&mut self, node: NodeId) {
        if !self.layout_transitions.contains(&node) {
            self.layout_transitions.push(node);
        }
    }

    /// Drops a node's layout transition record.
    pub fn forget_layout_transition(&mut self, node: NodeId) {
        self.layout_transitions.retain(|&other| other != node);
    }

    /// Live nodes whose children animate toward new layout rects.
    pub fn layout_transitions(&self) -> Vec<NodeId> {
        if self.layout_transitions.is_empty() {
            return Vec::new();
        }
        self.layout_transitions
            .iter()
            .copied()
            .filter(|id| self.is_valid(*id))
            .collect()
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
    pub fn node_absolute_rect(&self, id: NodeId) -> ResolvedRect {
        if !self.is_valid(id) {
            return ResolvedRect::ZERO;
        }
        let pt = self.node_absolute_point(id);
        let node = self.get(id);
        ResolvedRect::new(
            pt.x,
            pt.y,
            node.resolved_rect.width,
            node.resolved_rect.height,
        )
    }
}

impl Default for TreeArena {
    fn default() -> Self {
        Self::new()
    }
}
