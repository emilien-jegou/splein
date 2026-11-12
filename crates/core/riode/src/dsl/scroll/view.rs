// Single responsibility: High-level ScrollView component assembly and hook state registration.

use std::cell::Cell;
use std::rc::Rc;
use crate::reactive::Readable;
use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::dsl::flip::use_ref;
use crate::dsl::group::group;
use crate::foundation::{Direction, Material, Size};
use crate::prelude::component;
use crate::reactive::hooks::{current_scope, next_slot, no_scope, recall_value, remember_value};
use crate::reactive::use_signal;
use super::*;
use super::clamp::{ScrollClamp, ScrollGesture, ScrollPump};
use super::gesture::{scroll_thumb, scroll_viewport_group};

#[component]
pub fn ScrollView(
    width: Size,
    height: Size,
    direction: Option<Direction>,
    overscroll: Option<f32>,
    offset: Option<crate::reactive::Signal<f32>>,
    handle: Option<ScrollHandle>,
    on_scroll: Option<OnScroll>,
    sticky: Option<Element>,
    scrollbar: Option<Scrollbar>,
    thumb_fill: Option<Material>,
    bounce: Option<bool>,
    children: Vec<Element>,
) -> impl IntoElement {
    let direction = direction.unwrap_or(Direction::Vertical);
    let overscroll = overscroll.unwrap_or(DEFAULT_OVERSCROLL);
    let scrollbar = scrollbar.unwrap_or(Scrollbar::Auto);
    let bounce = bounce.unwrap_or(true);

    let owned = use_scroll(0.0);
    let offset_sig = offset.or_else(|| handle.clone().map(|h| h.offset)).unwrap_or_else(|| owned.offset.clone());
    let max_sig = handle.map(|h| h.max).unwrap_or_else(|| owned.max.clone());
    let viewport_ref = use_ref();
    let content_ref = use_ref();
    let thumb_pos = use_signal(0.0f32);
    let thumb_size = use_signal(0.0f32);
    let thumb_travel = use_signal(0.0f32);
    let thumb_op = use_signal(0.0f32);

    let Some(scope) = current_scope() else { no_scope() };
    
    macro_rules! memo {
        ($t:ty, $init:expr) => {{
            let slot = next_slot(&scope);
            scope.is_stable().then(|| recall_value::<$t>(&scope, slot)).flatten().unwrap_or_else(|| {
                let fresh = $init;
                if scope.is_stable() { remember_value(&scope, slot, &fresh); }
                fresh
            })
        }};
    }

    let fling: Rc<Cell<f32>> = memo!(Rc<Cell<f32>>, Rc::new(Cell::new(0.0f32)));
    let gesture: Rc<ScrollGesture> = memo!(Rc<ScrollGesture>, Rc::new(ScrollGesture { start: Cell::new(0.0), locked_out: Cell::new(false) }));
    let held: Rc<Cell<bool>> = memo!(Rc<Cell<bool>>, Rc::new(Cell::new(false)));
    let soft: Rc<Cell<bool>> = memo!(Rc<Cell<bool>>, Rc::new(Cell::new(false)));
    let hovered: Rc<Cell<bool>> = memo!(Rc<Cell<bool>>, Rc::new(Cell::new(false)));

    let slot = next_slot(&scope);
    if scope.is_stable().then(|| recall_value::<Rc<ScrollClamp>>(&scope, slot)).flatten().is_none() {
        let cell = Rc::new(ScrollClamp {
            direction, offset: offset_sig.clone(), max: max_sig.clone(),
            fling: Rc::clone(&fling), held: Rc::clone(&held), bounce,
            viewport: viewport_ref.clone(), content: content_ref.clone(),
            thumb_pos: thumb_pos.clone(), thumb_size: thumb_size.clone(),
            thumb_travel: thumb_travel.clone(), thumb_op: thumb_op.clone(),
            idle: Cell::new(f32::INFINITY), thumbs: scrollbar != Scrollbar::Hidden,
            on_scroll, last_emitted: Cell::new(f32::NAN), last_max: Cell::new(0.0),
            last_op: Cell::new(0.0), reveal: Cell::new(0.0), primed: Cell::new(false),
            was_beyond: Cell::new(false), soft: Rc::clone(&soft), hovered: Rc::clone(&hovered),
        });
        if scope.is_stable() { remember_value(&scope, slot, &cell); }
        offset_sig.attach(Box::new(ScrollPump { cell: Rc::downgrade(&cell) }));
        scope.runtime().borrow_mut().register_post_layout(move |dt| cell.resolve(dt));
    }

    let mut content_group = group().absolute().isolate_children(true).direction(direction);
    content_group = match direction {
        Direction::Vertical => content_group.scroll_offset_y(offset_sig.clone()).width(Size::Percent(1.0)),
        Direction::Horizontal => content_group.scroll_offset_x(offset_sig.clone()).height(Size::Percent(1.0)),
    };
    content_group = content_group.bind_ref(content_ref).children(children);

    let viewport = scroll_viewport_group(
        width, height, direction, overscroll, viewport_ref.clone(),
        &offset_sig, &max_sig, &fling, &held, &gesture, &viewport_ref,
        &thumb_pos, &thumb_size, bounce, &soft, &hovered,
    ).children([content_group.into_element()]);

    let viewport = match sticky {
        Some(pinned) => viewport.children([group().absolute().children([pinned]).into_element()]),
        None => viewport,
    };
    if scrollbar == Scrollbar::Hidden {
        return viewport.into_element();
    }
    viewport.children([scroll_thumb(
        direction, &offset_sig, &max_sig, &fling, &held, thumb_fill,
        &thumb_pos, &thumb_size, &thumb_travel, &thumb_op, bounce, &soft, &hovered,
    )]).into_element()
}
