// Single responsibility: Leaf element reconciliation for typography and custom primitives.

use std::cell::RefCell;
use std::rc::Rc;
use crate::dsl::cleanup::clean_orphaned_children;
use crate::dsl::custom::CustomDef;
use crate::dsl::reconciler::evaluate_prop;
use crate::dsl::text::TextDef;
use crate::foundation::Size;
use crate::reactive::ReactiveRuntime;
use crate::runtime::router::SubscriberRouter;
use crate::tree::binding::{DynamicBindings, NodeBindings};
use crate::tree::{DirtyFlags, LayoutNode, NodeId, NodeKind, TreeArena};

/// Reconciles text typography run and binds reactive text updates.
pub fn reconcile_text(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    def: TextDef,
) -> NodeId {
    let id = existing.unwrap_or_else(|| arena.insert(LayoutNode::default()));
    clean_orphaned_children(runtime, arena, router, id);

    let is_dynamic = def.content.is_dynamic();
    let m_sub = is_dynamic.then(|| router.measure_sub(id, &mut runtime.borrow_mut()));

    let store = DynamicBindings { text: def.content.dynamic_closure(), ..Default::default() };
    arena.get_mut(id).state.bindings = NodeBindings::from_store(store);

    let content = evaluate_prop(&Some(def.content.clone()), m_sub, runtime).unwrap_or_default();
    let config = def.to_config(content);

    let node_ref = arena.get(id);
    let changed = match &node_ref.kind {
        NodeKind::Text(t) => t != &config,
        _ => true,
    } || node_ref.style.margin != def.margin;

    if changed {
        arena.mark_dirty(id, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    }

    let effective_z = if def.z_index != 0 { def.z_index } else if def.is_absolute { 1 } else { 0 };
    let node = arena.get_mut(id);
    node.kind = NodeKind::Text(config);
    node.style.margin = def.margin;
    node.style.is_absolute = def.is_absolute;
    node.style.anchor = def.anchor;
    node.style.z_index = effective_z;
    node.style.is_overlay = def.is_overlay;
    node.style.has_layer = def.has_layer;
    id
}

/// Reconciles user-defined custom primitive elements.
pub fn reconcile_custom(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    def: CustomDef,
) -> NodeId {
    let id = existing.unwrap_or_else(|| arena.insert(LayoutNode::default()));
    clean_orphaned_children(runtime, arena, router, id);
    arena.get_mut(id).state.bindings = NodeBindings::default();

    let effective_z = if def.z_index != 0 { def.z_index } else if def.is_absolute { 1 } else { 0 };
    let node = arena.get_mut(id);
    node.kind = NodeKind::Custom(def.primitive);
    node.style.width = def.width.unwrap_or(Size::Fit);
    node.style.height = def.height.unwrap_or(Size::Fit);
    node.style.margin = def.margin;
    node.style.anchor = def.anchor;
    node.style.is_absolute = def.is_absolute;
    node.style.z_index = effective_z;
    node.style.is_overlay = def.is_overlay;
    node.style.has_layer = def.has_layer;
    arena.mark_dirty(id, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    id
}
