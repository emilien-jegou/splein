// Single responsibility: Element reconciliation dispatch and reactive property resolution.

use crate::dsl::element::Element;
use crate::dsl::reconcile_group::reconcile_group;
use crate::dsl::reconcile_leaf::{reconcile_custom, reconcile_text};
use crate::reactive::prop::DynamicProp;
use crate::reactive::{Observer, ReactiveRuntime, SubscriberId};
use crate::tree::SubscriberRouter;
use crate::tree::{NodeId, TreeArena};
use std::cell::RefCell;
use std::rc::Rc;

/// Reconciles an ephemeral Element tree against retained arena state.
#[tracing::instrument(skip_all)]
pub fn reconcile(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    element: Element,
) -> NodeId {
    match element {
        Element::Group(def) => reconcile_group(runtime, arena, router, existing, def),
        Element::Text(def) => reconcile_text(runtime, arena, router, existing, def),
        Element::Custom(def) => reconcile_custom(runtime, arena, router, existing, def),
    }
}

/// Evaluates a dynamic property while registering dependency subscriptions.
#[inline(always)]
pub fn evaluate_prop<T: Clone + 'static>(
    prop: &Option<DynamicProp<T>>,
    sub: Option<SubscriberId>,
    runtime: &Rc<RefCell<ReactiveRuntime>>,
) -> Option<T> {
    let prop = prop.as_ref()?;
    if let Some(sub) = sub {
        if prop.is_dynamic() {
            runtime
                .borrow_mut()
                .push_observer(Observer::Subscriber(sub));
            let val = prop.evaluate();
            runtime.borrow_mut().pop_observer();
            return Some(val);
        }
    }
    Some(prop.evaluate())
}
