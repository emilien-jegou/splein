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
    // Structural change dirties the parent: added/removed/reordered children shift
    // sibling allocation, so the parent must re-lay-out even when its own props are
    // unchanged. Without this the parent takes the layout cache path, which returns
    // without descending — fresh children are parentless until `set_children` below,
    // so their own upward invalidation never reaches it, and they keep zero rects
    // forever (invisible re-added subtrees). `replace_child` already marks likewise.
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

/// Reconciles one child, guarding context reads: a missing provider panics
/// during evaluation, and without this guard one unprovided consumer would
/// abort the whole walk. Flagged children (see
/// [`Element::has_context_reads`]) mount an empty group plus a console error
/// instead — the same contract as a panicking component body. The observer
/// stack is truncated back on unwind (manual push/pop pairs don't unwind).
fn reconcile_guarded(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    elem: Element,
) -> NodeId {
    if !elem.has_context_reads() {
        return reconcile(runtime, arena, router, existing, elem);
    }
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
