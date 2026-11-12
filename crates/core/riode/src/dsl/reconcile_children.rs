// Single responsibility: Child reconciliation applying pure diff operations to the arena.

use crate::dsl::cleanup::purge_node;
use crate::dsl::diff::{diff_children, ChildOp};
use crate::dsl::element::Element;
use crate::dsl::reconciler::reconcile;
use crate::reactive::ReactiveRuntime;
use crate::tree::SubscriberRouter;
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId};
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
    let old_specs: Vec<(NodeId, Option<_>)> = arena
        .children(parent)
        .iter()
        .map(|&id| (id, arena.get(id).key.clone()))
        .collect();

    let mut new_keys: Vec<Option<_>> = new_children.iter().map(|c| c.key()).collect();
    let ops = diff_children(&old_specs, &new_keys);

    let mut elements: Vec<Option<Element>> = new_children.into_iter().map(Some).collect();
    let mut resolved: Vec<Option<NodeId>> = (0..elements.len()).map(|_| None).collect();

    for op in ops {
        match op {
            ChildOp::Reuse { old, new_idx } => {
                let elem = elements[new_idx].take().expect("Child already consumed");
                let key = new_keys[new_idx].take();
                let child_id = reconcile_guarded(runtime, arena, router, Some(old), elem);
                arena.bind_key(key, child_id);
                resolved[new_idx] = Some(child_id);
            }
            ChildOp::Create { new_idx } => {
                let elem = elements[new_idx].take().expect("Child already consumed");
                let key = new_keys[new_idx].take();
                let child_id = reconcile_guarded(runtime, arena, router, None, elem);
                arena.bind_key(key, child_id);
                resolved[new_idx] = Some(child_id);
            }
            ChildOp::Remove(stale) => {
                purge_node(runtime, arena, router, stale);
            }
        }
    }

    let resolved: Vec<NodeId> = resolved
        .into_iter()
        .map(|slot| slot.expect("diff must cover every new child index"))
        .collect();

    let order_changed = old_specs.len() != resolved.len()
        || old_specs
            .iter()
            .zip(resolved.iter())
            .any(|((old, _), new)| old != new);
    arena.set_children(parent, resolved);
    if order_changed {
        arena.mark_dirty(parent, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    }
}

/// Reconciles one child, guarding against panics during evaluation (e.g. unprovided context).
fn reconcile_guarded(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    elem: Element,
) -> NodeId {
    let depth = runtime.borrow().observer_depth();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        reconcile(runtime, arena, router, existing, elem)
    }));
    match result {
        Ok(id) => id,
        Err(payload) => {
            runtime.borrow_mut().truncate_observer_stack(depth);
            tracing::error!(
                "riode: child reconcile failed ({}); mounting empty placeholder",
                crate::dsl::component::panic_message(&payload)
            );
            reconcile(
                runtime,
                arena,
                router,
                existing,
                crate::dsl::element::Element::Group(crate::dsl::group()),
            )
        }
    }
}
