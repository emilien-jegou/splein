// Single responsibility: Pure reconciliation diff algorithm between old and new child keys.

use crate::foundation::Key;
use crate::tree::NodeId;

/// Individual diffing action for child list reconciliation.
#[derive(Debug, PartialEq, Eq)]
pub enum ChildOp {
    /// Reuses an existing node for the element at target index.
    Reuse { old: NodeId, new_idx: usize },
    /// Creates a fresh node for the element at target index.
    Create { new_idx: usize },
    /// Removes an abandoned node.
    Remove(NodeId),
}

/// Computes pure list of reconciliation operations by matching keys or positional slots.
pub fn diff_children(
    old_children: &[(NodeId, Option<Key>)],
    new_keys: &[Option<Key>],
) -> Vec<ChildOp> {
    let all_old_unkeyed = old_children.iter().all(|(_, k)| k.is_none());
    let all_new_unkeyed = new_keys.iter().all(|k| k.is_none());
    if old_children.len() == new_keys.len() && all_old_unkeyed && all_new_unkeyed {
        return old_children
            .iter()
            .enumerate()
            .map(|(i, (id, _))| ChildOp::Reuse { old: *id, new_idx: i })
            .collect();
    }

    let mut ops = Vec::new();
    let mut old_keyed = std::collections::HashMap::new();
    let mut unkeyed_old = Vec::new();

    for &(id, ref key) in old_children {
        if let Some(k) = key {
            if let Some(displaced) = old_keyed.insert(k.clone(), id) {
                ops.push(ChildOp::Remove(displaced));
            }
        } else {
            unkeyed_old.push(id);
        }
    }

    let mut unkeyed_idx = 0;
    for (new_idx, key) in new_keys.iter().enumerate() {
        match key {
            Some(k) => {
                if let Some(old_id) = old_keyed.remove(k) {
                    ops.push(ChildOp::Reuse { old: old_id, new_idx });
                } else {
                    ops.push(ChildOp::Create { new_idx });
                }
            }
            None => {
                if let Some(&old_id) = unkeyed_old.get(unkeyed_idx) {
                    unkeyed_idx += 1;
                    ops.push(ChildOp::Reuse { old: old_id, new_idx });
                } else {
                    ops.push(ChildOp::Create { new_idx });
                }
            }
        }
    }

    for (_, stale) in old_keyed {
        ops.push(ChildOp::Remove(stale));
    }
    for &stale in &unkeyed_old[unkeyed_idx..] {
        ops.push(ChildOp::Remove(stale));
    }

    ops
}
