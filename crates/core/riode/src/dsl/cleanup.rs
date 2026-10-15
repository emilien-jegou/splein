// Single responsibility: Retained node removal and subscriber disposal helpers.

use crate::reactive::ReactiveRuntime;
use crate::tree::NodeId;
use crate::tree::SubscriberRouter;
use crate::tree::TreeArena;
use std::cell::RefCell;
use std::rc::Rc;

/// Detaches all children of a node and purges their retained resources and subscribers.
pub fn clean_orphaned_children(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    id: NodeId,
) {
    // Borrowed emptiness check + single owned copy: the old code built (and
    // for wide parents allocated) the child list twice — once to test empty,
    // once to iterate.
    if arena.each_child(id).next().is_none() {
        return;
    }
    let old_children = arena.children(id);
    for child in old_children {
        purge_node(runtime, arena, router, child);
    }
    arena.set_children(id, Vec::new());
}

/// Removes a node subtree from the arena and disposes all its bound reactive subscribers.
///
/// A keyed row leaving the tree also takes its hook state with it — React semantics: unmount
/// destroys state, re-adding mounts fresh. The node never carries the instance its hooks were
/// filed under (hooks allocate during the build, before the node exists), so the pairing was
/// recorded at build time and is read back here, per purged node, so removing a whole list
/// drops every row rather than only the container.
pub fn purge_node(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    node: NodeId,
) {
    let mut instances = Vec::new();
    arena.remove_with(node, |removed_id, key| {
        let orphaned_subs = router.unbind_node(removed_id);
        let mut rt = runtime.borrow_mut();
        for sub in orphaned_subs {
            rt.dispose_subscriber(sub);
        }
        if let Some(key) = key {
            if let Some(instance) = rt.element_hook_instance(&key) {
                // Unlink now: the pairing describes a live row, and the state it points at is
                // about to be dropped below.
                rt.unlink_element_hook_instance(&key);
                instances.push(instance);
            }
        }
    });
    // Outside the arena/runtime borrows: dropping a cell runs its teardown, which may read
    // or write signals, and disposes its effect through this same runtime.
    for instance in instances {
        crate::reactive::hooks::drop_instance_subtree_state(runtime, &instance);
    }
}
