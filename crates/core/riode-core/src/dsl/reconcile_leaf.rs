// Single responsibility: Unified leaf element reconciliation (text, images, and custom primitives).

use crate::dsl::cleanup::clean_orphaned_children;
use crate::dsl::leaf::LeafProps;
use crate::dsl::leaf_def::LeafDef;
use crate::foundation::Size;
use crate::reactive::{Observer, ReactiveRuntime};
use crate::tree::binding::NodeBindings;
use crate::tree::kind::LeafChange;
use crate::tree::SubscriberRouter;
use crate::tree::{DirtyFlags, NodeId, NodeKind};
use crate::tree::{LayoutNode, TreeArena};
use std::cell::RefCell;
use std::rc::Rc;

/// Prepares a leaf node for reconciliation: node identity, orphan cleanup, and
/// the evaluation node for context reads.
fn prepare_leaf(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
) -> NodeId {
    let id = existing.unwrap_or_else(|| arena.insert(LayoutNode::default()));
    clean_orphaned_children(runtime, arena, router, id);
    let _eval_node = crate::reactive::context::enter_node(id, arena);
    arena.invalidate_static_up(id);
    id
}

/// Applies the leaf's outer layout properties, returning the flags any style
/// drift requires (a leaf's box intent feeds measure, layout, and paint).
fn apply_leaf_style(arena: &mut TreeArena, id: NodeId, props: &LeafProps, extra: DirtyFlags) -> DirtyFlags {
    let node = arena.get(id);
    let style_changed = node.style.width != props.width.unwrap_or(Size::Fit)
        || node.style.height != props.height.unwrap_or(Size::Fit)
        || node.style.aspect_ratio != props.aspect_ratio
        || node.style.margin != props.margin
        || node.style.padding != props.padding
        || node.style.anchor != props.anchor
        || node.style.is_absolute != props.is_absolute
        || node.style.z_index != props.effective_z()
        || node.style.is_overlay != props.is_overlay
        || node.style.has_layer != props.has_layer;

    let effective_z = props.effective_z();
    let node = arena.get_mut(id);
    node.style.width = props.width.unwrap_or(Size::Fit);
    node.style.height = props.height.unwrap_or(Size::Fit);
    node.style.aspect_ratio = props.aspect_ratio;
    node.style.margin = props.margin;
    node.style.padding = props.padding;
    node.style.anchor = props.anchor;
    node.style.is_absolute = props.is_absolute;
    node.style.z_index = effective_z;
    node.style.is_overlay = props.is_overlay;
    node.style.has_layer = props.has_layer;

    if style_changed || extra != DirtyFlags::NONE {
        let flags = DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT | extra;
        arena.mark_dirty(id, flags);
        flags
    } else {
        DirtyFlags::NONE
    }
}

/// Reconciles any leaf element — the single path text, images, and third-party
/// primitives travel. The erased primitive absorbs the update in place when
/// the type matches; otherwise the instance is replaced wholesale.
pub fn reconcile_leaf(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    def: LeafDef,
) -> NodeId {
    let id = prepare_leaf(runtime, arena, router, existing);

    // Dynamic leaves subscribe for measure updates so content pushes re-run
    // the binding pass; static content never fires.
    let m_sub = def
        .dynamic
        .then(|| router.measure_sub(id, &mut runtime.borrow_mut()));

    let bindings = NodeBindings::from_store(def.bindings.clone());
    arena.get_mut(id).state.core.bindings = bindings.clone();

    let fresh = Rc::clone(&def.primitive);

    // Seed the resolved content on the fresh instance, with reactive reads
    // attributed to the measure subscriber (same contract as evaluate_prop).
    if let Some(sub) = m_sub {
        runtime.borrow_mut().push_observer(Observer::Subscriber(sub));
    }
    fresh.update_bindings(&bindings);
    if m_sub.is_some() {
        runtime.borrow_mut().pop_observer();
    }

    // Same-type absorb, else a flagged replacement.
    let old = arena.get(id).kind.primitive_rc();
    let change: LeafChange = match old.as_ref() {
        Some(previous) => previous.reconcile(&*fresh).unwrap_or(LeafChange {
            dirty: DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT,
            clear_selection: true,
        }),
        None => LeafChange {
            dirty: DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT,
            clear_selection: true,
        },
    };

    if change.clear_selection {
        arena.get_mut(id).state.text.text_selection = None;
    }
    apply_leaf_style(arena, id, &def.props, change.dirty);
    arena.get_mut(id).kind = NodeKind::Leaf(fresh);
    id
}
