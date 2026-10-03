// Single responsibility: Container group reconciliation and reactive binding attachment.

use crate::dsl::group::GroupDef;
use crate::dsl::reconcile_children::reconcile_children;
use crate::dsl::reconciler::evaluate_prop;
use crate::foundation::Transform;
use crate::reactive::ReactiveRuntime;
use crate::tree::binding::{DynamicBindings, NodeBindings};
use crate::tree::SubscriberRouter;
use crate::tree::{DirtyFlags, LayoutNode, NodeId, NodeKind, TreeArena};
use std::cell::RefCell;
use std::rc::Rc;

/// Reconciles flex group container properties, styles, and children into the arena.
pub fn reconcile_group(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    def: GroupDef,
) -> NodeId {
    let id = existing.unwrap_or_else(|| arena.insert(LayoutNode::default()));

    let has_dyn_measure = def.width.as_ref().is_some_and(|p| p.is_dynamic())
        || def.height.as_ref().is_some_and(|p| p.is_dynamic());
    let has_dyn_layout = def.transform.as_ref().is_some_and(|p| p.is_dynamic());
    let has_dyn_paint = def.fill.as_ref().is_some_and(|p| p.is_dynamic())
        || def.opacity.as_ref().is_some_and(|p| p.is_dynamic())
        || def.dynamic_shadow.as_ref().is_some_and(|p| p.is_dynamic());

    let m_sub = has_dyn_measure.then(|| router.measure_sub(id, &mut runtime.borrow_mut()));
    let l_sub = has_dyn_layout.then(|| router.layout_sub(id, &mut runtime.borrow_mut()));
    let p_sub = has_dyn_paint.then(|| router.paint_sub(id, &mut runtime.borrow_mut()));

    let store = DynamicBindings {
        width: def.width.as_ref().and_then(|p| p.dynamic_closure()),
        height: def.height.as_ref().and_then(|p| p.dynamic_closure()),
        fill: def.fill.as_ref().and_then(|p| p.dynamic_closure()),
        opacity: def.opacity.as_ref().and_then(|p| p.dynamic_closure()),
        transform: def.transform.as_ref().and_then(|p| p.dynamic_closure()),
        shadows: def
            .dynamic_shadow
            .as_ref()
            .and_then(|p| p.dynamic_closure())
            .map(|s| Rc::new(move || vec![s()]) as _),
        text: None,
    };
    arena.get_mut(id).state.bindings = NodeBindings::from_store(store);

    let new_w = evaluate_prop(&def.width, m_sub, runtime);
    let new_h = evaluate_prop(&def.height, m_sub, runtime);
    let new_tx = evaluate_prop(&def.transform, l_sub, runtime).unwrap_or(Transform::IDENTITY);
    let prev = arena.get(id);

    let layout_changed = (new_w.is_some() && new_w != Some(prev.style.width))
        || (new_h.is_some() && new_h != Some(prev.style.height))
        || prev.transform != new_tx
        || prev.style.layout != def.layout
        || prev.style.margin != def.margin
        || prev.style.clip != def.clip;

    if layout_changed {
        arena.mark_dirty(id, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    }

    let effective_z = if def.z_index != 0 {
        def.z_index
    } else if def.is_absolute {
        1
    } else {
        0
    };
    let node = arena.get_mut(id);
    node.kind = NodeKind::Group;
    if let Some(w) = new_w {
        node.style.width = w;
    }
    if let Some(h) = new_h {
        node.style.height = h;
    }
    node.transform = new_tx;
    node.style.layout = def.layout;
    node.style.margin = def.margin;
    node.style.clip = def.clip;
    node.style.is_absolute = def.is_absolute;
    node.style.anchor = def.anchor;
    node.style.z_index = effective_z;
    node.style.is_overlay = def.is_overlay;
    node.style.has_layer = def.has_layer;

    let new_fill = evaluate_prop(&def.fill, p_sub, runtime).flatten();
    let new_op = evaluate_prop(&def.opacity, p_sub, runtime);
    let new_shadow = evaluate_prop(&def.dynamic_shadow, p_sub, runtime);
    let mut resolved_shadows = def.shadows;
    if let Some(s) = new_shadow {
        resolved_shadows.push(s);
    }

    let prev_app = &arena.get(id).style.appearance;
    let paint_changed = prev_app.fill != new_fill
        || (new_op.is_some() && new_op != Some(prev_app.opacity))
        || prev_app.shadows != resolved_shadows
        || prev_app.stroke != def.stroke
        || prev_app.radius != def.radius;

    if paint_changed {
        arena.mark_dirty(id, DirtyFlags::PAINT);
    }

    let node = arena.get_mut(id);
    if let Some(f) = new_fill {
        node.style.appearance.fill = Some(f);
    }
    if let Some(op) = new_op {
        node.style.appearance.opacity = op;
    }
    node.style.appearance.stroke = def.stroke;
    node.style.appearance.shadows = resolved_shadows;
    node.style.appearance.radius = def.radius;

    reconcile_children(runtime, arena, router, id, def.children);
    id
}
