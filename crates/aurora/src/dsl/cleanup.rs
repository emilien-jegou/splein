// Single responsibility: Retained node removal and subscriber disposal helpers.

use crate::reactive::ReactiveRuntime;
use crate::tree::SubscriberRouter;
use crate::tree::{NodeId, TreeArena};
use std::cell::RefCell;
use std::rc::Rc;

/// Detaches all children of a node and purges their retained resources and subscribers.
pub fn clean_orphaned_children(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    id: NodeId,
) {
    if arena.children(id).is_empty() {
        return;
    }
    let old_children = arena.children(id).to_vec();
    for child in old_children {
        purge_node(runtime, arena, router, child);
    }
    arena.set_children(id, Vec::new());
}

/// Removes a node subtree from the arena and disposes all its bound reactive subscribers.
pub fn purge_node(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    node: NodeId,
) {
    arena.remove_with(node, |removed_id| {
        let orphaned_subs = router.unbind_node(removed_id);
        let mut rt = runtime.borrow_mut();
        for sub in orphaned_subs {
            rt.dispose_subscriber(sub);
        }
    });
}
