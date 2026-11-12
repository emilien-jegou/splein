// Single responsibility: Interaction handlers and scrollbar thumb rendering for scroll viewports.

use std::cell::Cell;
use std::rc::Rc;
use crate::dsl::element::Element;
use crate::dsl::element_ref::ElementRef;
use crate::dsl::group::{group, GroupElement};
use crate::foundation::{Anchor, Direction, Material, Size};
use crate::dsl::component::IntoElement;
use crate::input::{ClickInfo, DragInfo, HoverInfo, KeyCode, KeyEvent, PointerEvent, Propagation, WheelInfo};
use crate::reactive::Signal;
use super::*;
use super::clamp::ScrollGesture;

pub(crate) fn scroll_viewport_group(
    width: Size,
    height: Size,
    direction: Direction,
    overscroll: f32,
    viewport_ref: ElementRef,
    offset_sig: &Signal<f32>,
    max_sig: &Signal<f32>,
    fling: &Rc<Cell<f32>>,
    held: &Rc<Cell<bool>>,
    gesture: &Rc<ScrollGesture>,
    key_view_ref: &ElementRef,
    thumb_pos_sig: &Signal<f32>,
    thumb_size_sig: &Signal<f32>,
    bounce: bool,
    soft: &Rc<Cell<bool>>,
    hovered: &Rc<Cell<bool>>,
) -> GroupElement {
    let offset = offset_sig.clone();
    let fling_start = Rc::clone(fling);
    let held_start = Rc::clone(held);
    let gesture_start = Rc::clone(gesture);
    let offset_step = offset_sig.clone();
    let max_step = max_sig.clone();
    let gesture_step = Rc::clone(gesture);
    let fling_end = Rc::clone(fling);
    let held_end = Rc::clone(held);
    let gesture_end = Rc::clone(gesture);
    let offset_wheel = offset_sig.clone();
    let max_wheel = max_sig.clone();
    let fling_wheel = Rc::clone(fling);
    let soft_wheel = Rc::clone(soft);
    let offset_key = offset_sig.clone();
    let max_key = max_sig.clone();
    let fling_key = Rc::clone(fling);
    let key_view = key_view_ref.clone();
    let hover_enter = Rc::clone(hovered);
    let hover_move = Rc::clone(hovered);
    let hover_leave = Rc::clone(hovered);

    group()
        .width(width)
        .height(height)
        .clip(true)
        .scrollable(true)
        .overscroll(overscroll)
        .direction(direction)
        .bind_ref(viewport_ref)
        .on_pointer_enter(move |_: &HoverInfo| hover_enter.set(true))
        .on_pointer_move(move |_: &PointerEvent| hover_move.set(true))
        .on_pointer_leave(move |_: &HoverInfo| hover_leave.set(false))
        .on_drag_start_handled(move |_info: &DragInfo| {
            gesture_start.start.set(offset.get());
            gesture_start.locked_out.set(false);
            fling_start.set(0.0);
            held_start.set(true);
            Propagation::Stop
        })
        .on_drag_step_handled(move |info: &DragInfo| {
            if gesture_step.locked_out.get() { return Propagation::Stop; }
            let total = info.delta;
            let (main_delta, cross_delta) = match direction {
                Direction::Vertical => (total.y, total.x),
                Direction::Horizontal => (total.x, total.y),
            };
            if cross_delta.abs() > main_delta.abs() {
                gesture_step.locked_out.set(true);
                return Propagation::Stop;
            }
            let max = max_step.get();
            let raw = gesture_step.start.get() - main_delta;
            offset_step.set(if bounce { rubber_band(raw, 0.0, max) } else { raw.clamp(0.0, max) });
            Propagation::Stop
        })
        .on_drag_end_handled(move |info: &DragInfo| {
            let locked_out = gesture_end.locked_out.get();
            gesture_end.locked_out.set(false);
            held_end.set(false);
            if !locked_out {
                let vel = match direction {
                    Direction::Vertical => -info.velocity.y,
                    Direction::Horizontal => -info.velocity.x,
                };
                fling_end.set(vel.clamp(-FLING_MAX, FLING_MAX));
            }
            Propagation::Stop
        })
        .on_wheel_handled(move |wheel: &WheelInfo| {
            fling_wheel.set(0.0);
            let max = max_wheel.get();
            if max <= 0.0 { return Propagation::Stop; }
            let delta = match direction {
                Direction::Vertical => wheel.delta_y,
                Direction::Horizontal => if wheel.delta_x != 0.0 { wheel.delta_x } else { wheel.delta_y },
            };
            let raw = offset_wheel.get() - delta;
            soft_wheel.set(true);
            offset_wheel.set(if bounce { rubber_band(raw, 0.0, max) } else { raw.clamp(0.0, max) });
            Propagation::Stop
        })
        .on_key_handled(move |key: &KeyEvent| {
            fling_key.set(0.0);
            let max = max_key.get().max(0.0);
            let current = offset_key.get();
            let page = match direction {
                Direction::Vertical => (key_view.rect().map(|r| r.height).unwrap_or(600.0) - 48.0).max(64.0),
                Direction::Horizontal => (key_view.rect().map(|r| r.width).unwrap_or(800.0) - 48.0).max(64.0),
            };
            let next = match (direction, key.code) {
                (Direction::Vertical, KeyCode::Up) => current - SCROLL_LINE,
                (Direction::Vertical, KeyCode::Down) => current + SCROLL_LINE,
                (Direction::Horizontal, KeyCode::Left) => current - SCROLL_LINE,
                (Direction::Horizontal, KeyCode::Right) => current + SCROLL_LINE,
                (_, KeyCode::PageUp) => current - page,
                (_, KeyCode::PageDown) => current + page,
                (_, KeyCode::Home) => 0.0,
                (_, KeyCode::End) => max,
                (_, KeyCode::Char(' ')) => if key.shift { current - page } else { current + page },
                _ => return Propagation::Bubble,
            };
            offset_key.set(next.clamp(0.0, max));
            Propagation::Stop
        })
        .on_click({
            let offset = offset_sig.clone();
            let max = max_sig.clone();
            let fling = Rc::clone(fling);
            let view = key_view_ref.clone();
            let thumb_pos = thumb_pos_sig.clone();
            let thumb_size = thumb_size_sig.clone();
            move |info: &ClickInfo| {
                fling.set(0.0);
                let max = max.get();
                if max <= 0.0 { return; }
                let Some(v) = view.rect() else { return };
                let (edge, size, pos, _is_vert) = match direction {
                    Direction::Vertical => (info.position.x >= v.x + v.width - TRACK_ZONE, v.height, info.position.y - (v.y + thumb_pos.get()), true),
                    Direction::Horizontal => (info.position.y >= v.y + v.height - TRACK_ZONE, v.width, info.position.x - (v.x + thumb_pos.get()), false),
                };
                if !edge { return; }
                let page = (size - 48.0).max(64.0);
                let center = thumb_size.get() * 0.5;
                let next = if pos < center { offset.get() - page } else { offset.get() + page };
                offset.set(next.clamp(0.0, max));
            }
        })
}

pub(crate) fn scroll_thumb(
    direction: Direction,
    offset_sig: &Signal<f32>,
    max_sig: &Signal<f32>,
    fling: &Rc<Cell<f32>>,
    held: &Rc<Cell<bool>>,
    thumb_fill: Option<Material>,
    thumb_pos: &Signal<f32>,
    thumb_size: &Signal<f32>,
    thumb_travel: &Signal<f32>,
    thumb_op: &Signal<f32>,
    bounce: bool,
    soft: &Rc<Cell<bool>>,
    hovered: &Rc<Cell<bool>>,
) -> Element {
    let fling = Rc::clone(fling);
    let offset = offset_sig.clone();
    let grab = Rc::new(Cell::new(0.0f32));
    let grab_step = Rc::clone(&grab);
    let offset_step = offset_sig.clone();
    let max_step = max_sig.clone();
    let soft_step = Rc::clone(soft);
    let travel_step = thumb_travel.clone();
    let thumb_size_step = thumb_size.clone();
    let fling_end = Rc::clone(&fling);
    let held_start = Rc::clone(held);
    let held_end = Rc::clone(held);
    let max_end = max_sig.clone();
    let travel_end = thumb_travel.clone();
    let max_visible = max_sig.clone();
    let op_visible = thumb_op.clone();
    let hover_thumb_enter = Rc::clone(hovered);
    let hover_thumb_move = Rc::clone(hovered);

    let base = match direction {
        Direction::Vertical => group()
            .absolute().anchor(Anchor::TopRight)
            .width(THUMB_WIDTH).height(thumb_size.clone())
            .x(-THUMB_INSET).y(thumb_pos.clone()),
        Direction::Horizontal => group()
            .absolute().anchor(Anchor::BottomLeft)
            .height(THUMB_WIDTH).width(thumb_size.clone())
            .y(-THUMB_INSET).x(thumb_pos.clone()),
    };

    base.material_opt(thumb_fill)
        .visible(move || max_visible.get() > 0.0 && op_visible.get() > 0.0)
        .on_click_handled(move |_| Propagation::Stop)
        .on_pointer_enter(move |_: &HoverInfo| hover_thumb_enter.set(true))
        .on_pointer_move(move |_: &PointerEvent| hover_thumb_move.set(true))
        .on_drag_start_handled(move |_info: &DragInfo| {
            grab.set(offset.get());
            fling.set(0.0);
            held_start.set(true);
            Propagation::Stop
        })
        .on_drag_step_handled(move |info: &DragInfo| {
            let (travel, max) = (travel_step.get(), max_step.get());
            if travel > 0.0 && max > 0.0 {
                let delta = match direction {
                    Direction::Vertical => info.delta.y,
                    Direction::Horizontal => info.delta.x,
                };
                let raw = grab_step.get() + delta / travel * max;
                let view_extent = travel + thumb_size_step.get();
                let (lo, hi) = (-0.5 * view_extent, max + 0.5 * view_extent);
                soft_step.set(true);
                offset_step.set(if bounce { rubber_band(raw, 0.0, max).clamp(lo, hi) } else { raw.clamp(0.0, max) });
            }
            Propagation::Stop
        })
        .on_drag_end_handled(move |info: &DragInfo| {
            held_end.set(false);
            let (travel, max) = (travel_end.get(), max_end.get());
            if travel > 0.0 && max > 0.0 {
                let vel = match direction {
                    Direction::Vertical => info.velocity.y,
                    Direction::Horizontal => info.velocity.x,
                };
                fling_end.set((vel / travel * max).clamp(-FLING_MAX, FLING_MAX));
            }
            Propagation::Stop
        })
        .into_element()
}
