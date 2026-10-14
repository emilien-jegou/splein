// Single responsibility: Bidirectional index resolving stable element keys to retained nodes.

use rustc_hash::FxHashMap;

use crate::foundation::Key;
use crate::tree::id::NodeId;

/// Bidirectional index pairing stable element keys with retained node handles.
#[derive(Default)]
pub struct KeyRegistry {
    by_key: FxHashMap<Key, NodeId>,
    by_node: FxHashMap<NodeId, Key>,
}

impl KeyRegistry {
    /// Creates an empty key index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the handle recorded for a key, without checking liveness.
    #[inline(always)]
    pub fn resolve(&self, key: &Key) -> Option<NodeId> {
        self.by_key.get(key).copied()
    }

    /// Pairs a node with a key, releasing both sides of any stale pairing.
    pub fn bind(&mut self, key: Key, node: NodeId) {
        self.unbind(node);
        if let Some(displaced) = self.by_key.insert(key.clone(), node) {
            self.by_node.remove(&displaced);
        }
        self.by_node.insert(node, key);
    }

    /// Drops every key held by a node leaving the arena.
    #[inline(always)]
    pub fn unbind(&mut self, node: NodeId) {
        if let Some(key) = self.by_node.remove(&node) {
            self.by_key.remove(&key);
        }
    }
}
