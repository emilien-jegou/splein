// Single responsibility: Child reconciliation applying pure diff operations to the arena.

use crate::dsl::cleanup::purge_node;
use crate::dsl::diff::{diff_children, ChildOp};
use crate::dsl::element::Element;
use crate::dsl::reconciler::reconcile;
use aurora_reactive::ReactiveRuntime;
use crate::tree::SubscriberRouter;
use crate::tree::{NodeId};
use crate::tree::{TreeArena};
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
    // Old specs straight from the arena slice: one Vec, one key clone per
    // child, no intermediate id list.
    let old_specs: Vec<(NodeId, Option<_>)> = arena
        .children(parent)
        .iter()
        .map(|&id| (id, arena.get(id).key.clone()))
        .collect();

    let mut new_keys: Vec<Option<_>> = new_children.iter().map(|c| c.key()).collect();
    let ops = diff_children(&old_specs, &new_keys);

    let mut elements: Vec<Option<Element>> = new_children.into_iter().map(Some).collect();
    // Indexed resolve: every new index gets exactly one Reuse/Create op, so
    // placement is O(n) with no sort. Keys move out of `new_keys` instead
    // of being re-queried (and re-cloned) off the element.
    let mut resolved: Vec<Option<NodeId>> = (0..elements.len()).map(|_| None).collect();

    for op in ops {
        match op {
            ChildOp::Reuse { old, new_idx } => {
                let elem = elements[new_idx].take().expect("Child already consumed");
                let key = new_keys[new_idx].take();
                let child_id = reconcile(runtime, arena, router, Some(old), elem);
                arena.bind_key(key, child_id);
                resolved[new_idx] = Some(child_id);
            }
            ChildOp::Create { new_idx } => {
                let elem = elements[new_idx].take().expect("Child already consumed");
                let key = new_keys[new_idx].take();
                let child_id = reconcile(runtime, arena, router, None, elem);
                arena.bind_key(key, child_id);
                resolved[new_idx] = Some(child_id);
            }
            ChildOp::Remove(stale) => {
                purge_node(runtime, arena, router, stale);
            }
        }
    }

    let resolved = resolved
        .into_iter()
        .map(|slot| slot.expect("diff must cover every new child index"))
        .collect();
    arena.set_children(parent, resolved);
}
