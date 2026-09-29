// Single responsibility: Child reconciliation applying pure diff operations to the arena.

use crate::dsl::cleanup::purge_node;
use crate::dsl::diff::{diff_children, ChildOp};
use crate::dsl::element::Element;
use crate::dsl::reconciler::reconcile;
use crate::reactive::ReactiveRuntime;
use crate::runtime::SubscriberRouter;
use crate::tree::{NodeId, TreeArena};
use std::cell::RefCell;
use std::rc::Rc;

/// Reconciles child element lists using pure diffing and applies changes to the arena.
pub fn reconcile_children(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    parent: NodeId,
    new_children: Vec<Element>,
) {
    let old_child_ids = arena.children(parent).to_vec();
    let old_specs: Vec<(NodeId, Option<_>)> = old_child_ids
        .iter()
        .map(|&id| (id, arena.get(id).key.clone()))
        .collect();

    let new_keys: Vec<Option<_>> = new_children.iter().map(|c| c.key()).collect();
    let ops = diff_children(&old_specs, &new_keys);

    let mut elements: Vec<Option<Element>> = new_children.into_iter().map(Some).collect();
    let mut final_children: Vec<(usize, NodeId)> = Vec::new();

    for op in ops {
        match op {
            ChildOp::Reuse { old, new_idx } => {
                let elem = elements[new_idx].take().expect("Child already consumed");
                let key = elem.key();
                let child_id = reconcile(runtime, arena, router, Some(old), elem);
                arena.get_mut(child_id).key = key;
                final_children.push((new_idx, child_id));
            }
            ChildOp::Create { new_idx } => {
                let elem = elements[new_idx].take().expect("Child already consumed");
                let key = elem.key();
                let child_id = reconcile(runtime, arena, router, None, elem);
                arena.get_mut(child_id).key = key;
                final_children.push((new_idx, child_id));
            }
            ChildOp::Remove(stale) => {
                purge_node(runtime, arena, router, stale);
            }
        }
    }

    final_children.sort_by_key(|&(idx, _)| idx);
    let resolved = final_children.into_iter().map(|(_, id)| id).collect();
    arena.set_children(parent, resolved);
}
