// Single responsibility: Join table binding reactive subscribers to arena nodes and dirty categories.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::reactive::{ReactiveRuntime, SubscriberId};
use crate::tree::flags::DirtyFlags;
use crate::tree::id::NodeId;

/// Manages subscriber allocation and invalidation routing for arena nodes.
#[derive(Default)]
pub struct SubscriberRouter {
    subscriber_to_node: FxHashMap<SubscriberId, (NodeId, DirtyFlags)>,
    node_to_subscribers: FxHashMap<NodeId, SmallVec<[SubscriberId; 2]>>,
    node_measure_subs: FxHashMap<NodeId, SubscriberId>,
    node_layout_subs: FxHashMap<NodeId, SubscriberId>,
    node_paint_subs: FxHashMap<NodeId, SubscriberId>,
}

impl SubscriberRouter {
    /// Creates a new subscriber routing table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Obtains or allocates a measurement subscriber for a node.
    pub fn measure_sub(&mut self, node: NodeId, runtime: &mut ReactiveRuntime) -> SubscriberId {
        if let Some(&sub) = self.node_measure_subs.get(&node) {
            return sub;
        }
        let sub = runtime.alloc_subscriber();
        self.node_measure_subs.insert(node, sub);
        self.bind(sub, node, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
        sub
    }

    /// Obtains or allocates a layout-only subscriber.
    pub fn layout_sub(&mut self, node: NodeId, runtime: &mut ReactiveRuntime) -> SubscriberId {
        if let Some(&sub) = self.node_layout_subs.get(&node) {
            return sub;
        }
        let sub = runtime.alloc_subscriber();
        self.node_layout_subs.insert(node, sub);
        self.bind(sub, node, DirtyFlags::LAYOUT);
        sub
    }

    /// Obtains or allocates a paint subscriber for a node.
    pub fn paint_sub(&mut self, node: NodeId, runtime: &mut ReactiveRuntime) -> SubscriberId {
        if let Some(&sub) = self.node_paint_subs.get(&node) {
            return sub;
        }
        let sub = runtime.alloc_subscriber();
        self.node_paint_subs.insert(node, sub);
        self.bind(sub, node, DirtyFlags::PAINT);
        sub
    }

    /// Binds an existing subscriber handle to an arena node and category flags.
    pub fn bind(&mut self, sub: SubscriberId, node: NodeId, flags: DirtyFlags) {
        self.subscriber_to_node.insert(sub, (node, flags));
        self.node_to_subscribers.entry(node).or_default().push(sub);
    }

    /// Resolves which node and dirty category corresponds to a subscriber.
    #[inline(always)]
    pub fn lookup(&self, sub: SubscriberId) -> Option<(NodeId, DirtyFlags)> {
        self.subscriber_to_node.get(&sub).copied()
    }

    /// Unbinds and purges all subscribers associated with a node.
    pub fn unbind_node(&mut self, node: NodeId) -> Vec<SubscriberId> {
        self.node_measure_subs.remove(&node);
        self.node_layout_subs.remove(&node);
        self.node_paint_subs.remove(&node);
        if let Some(subs) = self.node_to_subscribers.remove(&node) {
            for s in &subs {
                self.subscriber_to_node.remove(s);
            }
            subs.to_vec()
        } else {
            Vec::new()
        }
    }
}
