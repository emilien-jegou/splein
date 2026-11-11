// Single responsibility: Client-side scroll viewport — clip box + absolute content + clamped offset.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::dsl::element_ref::ElementRef;
use crate::dsl::flip::use_ref;
use crate::dsl::group::group;
use crate::foundation::{Anchor, Direction, Margin, Material, Size};
use crate::input::{ClickInfo, DragInfo, HoverInfo, KeyCode, KeyEvent, PointerEvent, Propagation, WheelInfo};
use crate::prelude::component;
use crate::reactive::hooks::{current_scope, next_slot, no_scope, recall_value, remember_value};
use crate::reactive::{use_signal, Readable, Signal, Ticker};

pub const DEFAULT_OVERSCROLL: f32 = 512.0;
pub const SCROLL_LINE: f32 = 40.0;
const FLING_DECAY: f32 = 7.0;
const FLING_STOP: f32 = 24.0;
const FLING_MAX: f32 = 4000.0;
const THUMB_MIN: f32 = 24.0;
const THUMB_WIDTH: f32 = 8.0;
const THUMB_INSET: f32 = 4.0;
const THUMB_EDGE: f32 = 6.0;
const THUMB_ALPHA: f32 = 0.5;
const FADE_DELAY: f32 = 1.0;
const FADE_IN: f32 = 0.2;
const FADE_TIME: f32 = 0.3;
const TRACK_ZONE: f32 = 24.0;
const RUBBER_BAND: f32 = 120.0;
const SPRING_K: f32 = 180.0;
const SPRING_C: f32 = 22.0;
const SPRING_X_EPS: f32 = 0.5;
const SPRING_V_EPS: f32 = 30.0;

fn rubber_band(raw: f32, min: f32, max: f32) -> f32 {
    let clamped = raw.clamp(min, max);
    let over = raw - clamped;
    if over == 0.0 {
        clamped
    } else {
        clamped + over / (1.0 + over.abs() / RUBBER_BAND)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Scrollbar {
    #[default]
    Auto,
    Visible,
    Hidden,
}

#[derive(Clone)]
pub struct ScrollHandle {
    pub offset: Signal<f32>,
    pub max: Signal<f32>,
}

impl ScrollHandle {
    pub fn scroll_to(&self, offset: f32) {
        self.offset.set(offset);
    }

    pub fn scroll_by(&self, delta: f32) {
        self.offset.set(self.offset.get() + delta);
    }

    pub fn scroll_to_index(&self, index: usize, row_size: f32) {
        self.offset.set(index as f32 * row_size.max(1.0));
    }

    pub fn scroll_into_view(&self, rect_pos: f32, rect_size: f32, view_size: f32) {
        let cur = self.offset.get();
        let next = if rect_pos < cur {
            rect_pos
        } else if rect_pos + rect_size > cur + view_size {
            rect_pos + rect_size - view_size
        } else {
            return;
        };
        self.offset.set(next.clamp(0.0, self.max.get().max(0.0)));
    }
}

pub fn use_scroll(initial: f32) -> ScrollHandle {
    ScrollHandle {
        offset: use_signal(initial),
        max: use_signal(0.0),
    }
}

pub struct OnScroll(pub Rc<dyn Fn(f32)>);

impl<F: Fn(f32) + 'static> From<F> for OnScroll {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

#[derive(Clone)]
pub struct RowFn(pub Rc<dyn Fn(usize) -> Element>);

impl<F: Fn(usize) -> Element + 'static> From<F> for RowFn {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

#[derive(Clone)]
struct ScrollGesture {
    start: Cell<f32>,
    locked_out: Cell<bool>,
}

struct ScrollPump {
    cell: Weak<ScrollClamp>,
}

impl ScrollPump {
    fn live(cell: &ScrollClamp) -> bool {
        if cell.fling.get() != 0.0 {
            return true;
        }
        let (off, max) = (cell.last_emitted.get(), cell.last_max.get());
        if off < 0.0 || off > max {
            return true;
        }
        if !(cell.thumbs && max > 0.0) {
            return false;
        }
        let op = cell.last_op.get();
        (op > 0.0 && op < THUMB_ALPHA) || cell.idle.get() < FADE_DELAY + FADE_TIME
    }
}

impl Ticker for ScrollPump {
    fn advance(&mut self, _dt: f32) -> bool {
        self.cell.upgrade().is_some()
    }

    fn is_active(&self) -> bool {
        self.cell.upgrade().is_some_and(|c| Self::live(&c))
    }
}

struct ScrollClamp {
    direction: Direction,
    offset: Signal<f32>,
    max: Signal<f32>,
    fling: Rc<Cell<f32>>,
    held: Rc<Cell<bool>>,
    bounce: bool,
    viewport: ElementRef,
    content: ElementRef,
    thumb_pos: Signal<f32>,
    thumb_size: Signal<f32>,
    thumb_travel: Signal<f32>,
    thumb_op: Signal<f32>,
    last_max: Cell<f32>,
    last_op: Cell<f32>,
    reveal: Cell<f32>,
    idle: Cell<f32>,
    hovered: Rc<Cell<bool>>,
    thumbs: bool,
    on_scroll: Option<OnScroll>,
    last_emitted: Cell<f32>,
    primed: Cell<bool>,
    was_beyond: Cell<bool>,
    soft: Rc<Cell<bool>>,
}

impl ScrollClamp {
    fn resolve_free(&self, dt: f32, max: f32) {
        let cur = self.offset.get();
        let beyond = cur < 0.0 || cur > max;
        let soft = self.soft.take();
        if beyond && !soft && !self.was_beyond.get() && self.fling.get() == 0.0 {
            self.offset.set(cur.clamp(0.0, max));
        } else if dt > 0.0 {
            if cur < 0.0 || cur > max {
                let target = cur.clamp(0.0, max);
                let mut x = cur - target;
                let mut v = self.fling.get();
                v += (-SPRING_K * x - SPRING_C * v) * dt;
                x += v * dt;
                if x.abs() < SPRING_X_EPS && v.abs() < SPRING_V_EPS {
                    self.offset.set(target);
                    self.fling.set(0.0);
                } else {
                    self.offset.set(target + x);
                    self.fling.set(v);
                }
            } else {
                let v = self.fling.get();
                if v != 0.0 {
                    self.offset.set(cur + v * dt);
                    let decayed = v * (-dt * FLING_DECAY).exp();
                    self.fling.set(if decayed.abs() < FLING_STOP {
                        0.0
                    } else {
                        decayed
                    });
                }
            }
        }
        let settled = self.offset.get();
        self.was_beyond.set(settled < 0.0 || settled > max);
    }

    fn resolve(&self, dt: f32) {
        let dt = dt.min(1.0 / 20.0);
        let (Some(view), Some(content)) = (self.viewport.rect(), self.content.rect()) else {
            return;
        };
        let (view_extent, content_extent) = match self.direction {
            Direction::Vertical => (view.height, content.height),
            Direction::Horizontal => (view.width, content.width),
        };
        let max = (content_extent - view_extent).max(0.0);
        self.max.set_if_changed(max);
        self.last_max.set(max);
        if !self.held.get() {
            if self.bounce {
                self.resolve_free(dt, max);
            } else {
                let v = self.fling.get();
                if v != 0.0 && dt > 0.0 {
                    self.offset.set(self.offset.get() + v * dt);
                    let decayed = v * (-dt * FLING_DECAY).exp();
                    self.fling.set(if decayed.abs() < FLING_STOP {
                        0.0
                    } else {
                        decayed
                    });
                }
                let clamped = self.offset.get().clamp(0.0, max);
                if clamped != self.offset.get() {
                    self.offset.set(clamped);
                    self.fling.set(0.0);
                }
            }
        }
        let current = self.offset.get();
        if !self.primed.get() {
            self.primed.set(true);
            self.last_emitted.set(current);
        } else if current != self.last_emitted.get() {
            self.last_emitted.set(current);
            self.idle.set(0.0);
            if let Some(cb) = &self.on_scroll {
                (cb.0)(current);
            }
        } else {
            self.idle.set(self.idle.get() + dt);
        }
        if self.thumbs {
            let lane = (view_extent - 2.0 * THUMB_EDGE).max(0.0);
            let thumb_size = if max <= 0.0 {
                0.0
            } else {
                (view_extent * view_extent / content_extent)
                    .clamp(THUMB_MIN, view_extent)
                    .min(lane)
            };
            self.thumb_size.set_if_changed(thumb_size);
            let travel = (lane - thumb_size).max(0.0);
            self.thumb_travel.set_if_changed(travel);
            let pos = if max <= 0.0 {
                0.0
            } else {
                THUMB_EDGE + current.clamp(0.0, max) / max * travel
            };
            self.thumb_pos.set_if_changed(pos);

            let target = if max <= 0.0 {
                0.0
            } else if self.hovered.get() {
                THUMB_ALPHA
            } else {
                let idle = self.idle.get();
                if idle < FADE_DELAY {
                    THUMB_ALPHA
                } else if idle >= FADE_DELAY + FADE_TIME {
                    0.0
                } else {
                    THUMB_ALPHA * (1.0 - (idle - FADE_DELAY) / FADE_TIME)
                }
            };

            let op = if target > 0.0 && self.last_op.get() == 0.0 {
                self.reveal.set(self.reveal.get() + dt);
                (THUMB_ALPHA * self.reveal.get() / FADE_IN).min(target)
            } else {
                if target == 0.0 {
                    self.reveal.set(0.0);
                }
                target
            };
            self.thumb_op.set_if_changed(op);
            self.last_op.set(op);
        }
    }
}

#[component]
pub fn ScrollView(
    width: Size,
    height: Size,
    direction: Option<Direction>,
    overscroll: Option<f32>,
    offset: Option<Signal<f32>>,
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
    let offset_sig = offset
        .or_else(|| handle.clone().map(|h| h.offset))
        .unwrap_or_else(|| owned.offset.clone());
    let max_sig = handle
        .map(|h| h.max)
        .unwrap_or_else(|| owned.max.clone());
    let viewport_ref = use_ref();
    let content_ref = use_ref();
    let thumb_pos = use_signal(0.0f32);
    let thumb_size = use_signal(0.0f32);
    let thumb_travel = use_signal(0.0f32);
    let thumb_op = use_signal(0.0f32);

    let Some(scope) = current_scope() else {
        no_scope()
    };
    let fling: Rc<Cell<f32>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<f32>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(0.0f32));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let gesture: Rc<ScrollGesture> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<ScrollGesture>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(ScrollGesture {
                    start: Cell::new(0.0),
                    locked_out: Cell::new(false),
                });
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let held: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let soft: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let hovered: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };

    let slot = next_slot(&scope);
    match scope
        .is_stable()
        .then(|| recall_value::<Rc<ScrollClamp>>(&scope, slot))
        .flatten()
    {
        Some(_) => {}
        None => {
            let cell = Rc::new(ScrollClamp {
                direction,
                offset: offset_sig.clone(),
                max: max_sig.clone(),
                fling: Rc::clone(&fling),
                held: Rc::clone(&held),
                bounce,
                viewport: viewport_ref.clone(),
                content: content_ref.clone(),
                thumb_pos: thumb_pos.clone(),
                thumb_size: thumb_size.clone(),
                thumb_travel: thumb_travel.clone(),
                thumb_op: thumb_op.clone(),
                idle: Cell::new(f32::INFINITY),
                thumbs: scrollbar != Scrollbar::Hidden,
                on_scroll,
                last_emitted: Cell::new(f32::NAN),
                last_max: Cell::new(0.0),
                last_op: Cell::new(0.0),
                reveal: Cell::new(0.0),
                primed: Cell::new(false),
                was_beyond: Cell::new(false),
                soft: Rc::clone(&soft),
                hovered: Rc::clone(&hovered),
            });
            if scope.is_stable() {
                remember_value(&scope, slot, &cell);
            }
            offset_sig.attach(Box::new(ScrollPump {
                cell: Rc::downgrade(&cell),
            }));
            scope
                .runtime()
                .borrow_mut()
                .register_post_layout(move |dt| cell.resolve(dt));
        }
    }

    let mut content_group = group()
        .absolute()
        .isolate_children(true)
        .direction(direction)
        .scroll_offset(offset_sig.clone())
        .bind_ref(content_ref)
        .children(children);

    content_group = match direction {
        Direction::Vertical => content_group.width(Size::Percent(1.0)),
        Direction::Horizontal => content_group.height(Size::Percent(1.0)),
    };

    let viewport = scroll_viewport_group(
        width,
        height,
        direction,
        overscroll,
        viewport_ref.clone(),
        &offset_sig,
        &max_sig,
        &fling,
        &held,
        &gesture,
        &viewport_ref,
        &thumb_pos,
        &thumb_size,
        bounce,
        &soft,
        &hovered,
    )
    .children([content_group.into_element()]);

    let viewport = match sticky {
        Some(pinned) => viewport
            .children([group().absolute().children([pinned]).into_element()]),
        None => viewport,
    };
    if scrollbar == Scrollbar::Hidden {
        return viewport.into_element();
    }
    viewport
        .children([scroll_thumb(
            direction,
            &offset_sig,
            &max_sig,
            &fling,
            &held,
            thumb_fill,
            &thumb_pos,
            &thumb_size,
            &thumb_travel,
            &thumb_op,
            bounce,
            &soft,
            &hovered,
        )])
        .into_element()
}

#[allow(clippy::too_many_arguments)]
fn scroll_viewport_group(
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
) -> crate::dsl::group::GroupDef {
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
        .on_pointer_enter(move |_: &HoverInfo| {
            hover_enter.set(true);
        })
        .on_pointer_move(move |_: &PointerEvent| {
            hover_move.set(true);
        })
        .on_pointer_leave(move |_: &HoverInfo| {
            hover_leave.set(false);
        })
        .on_drag_start_handled(move |_info: &DragInfo| {
            gesture_start.start.set(offset.get());
            gesture_start.locked_out.set(false);
            fling_start.set(0.0);
            held_start.set(true);
            Propagation::Stop
        })
        .on_drag_step_handled(move |info: &DragInfo| {
            if gesture_step.locked_out.get() {
                return Propagation::Stop;
            }
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
            offset_step.set(if bounce {
                rubber_band(raw, 0.0, max)
            } else {
                raw.clamp(0.0, max)
            });
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
            if max <= 0.0 {
                return Propagation::Stop;
            }
            let delta = match direction {
                Direction::Vertical => wheel.delta_y,
                Direction::Horizontal => {
                    if wheel.delta_x != 0.0 {
                        wheel.delta_x
                    } else {
                        wheel.delta_y
                    }
                }
            };
            let raw = offset_wheel.get() - delta;
            soft_wheel.set(true);
            offset_wheel.set(if bounce {
                rubber_band(raw, 0.0, max)
            } else {
                raw.clamp(0.0, max)
            });
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
                (_, KeyCode::Char(' ')) => {
                    if key.shift {
                        current - page
                    } else {
                        current + page
                    }
                }
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
                if max <= 0.0 {
                    return;
                }
                let Some(v) = view.rect() else { return };
                match direction {
                    Direction::Vertical => {
                        if info.position.x < v.x + v.width - TRACK_ZONE {
                            return;
                        }
                        let page = (v.height - 48.0).max(64.0);
                        let center = v.y + thumb_pos.get() + thumb_size.get() * 0.5;
                        let next = if info.position.y < center {
                            offset.get() - page
                        } else {
                            offset.get() + page
                        };
                        offset.set(next.clamp(0.0, max));
                    }
                    Direction::Horizontal => {
                        if info.position.y < v.y + v.height - TRACK_ZONE {
                            return;
                        }
                        let page = (v.width - 48.0).max(64.0);
                        let center = v.x + thumb_pos.get() + thumb_size.get() * 0.5;
                        let next = if info.position.x < center {
                            offset.get() - page
                        } else {
                            offset.get() + page
                        };
                        offset.set(next.clamp(0.0, max));
                    }
                }
            }
        })
}

fn scroll_thumb(
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
            .absolute()
            .anchor(Anchor::TopRight)
            .width(THUMB_WIDTH)
            .height(thumb_size.clone())
            .x(-THUMB_INSET)
            .y(thumb_pos.clone()),
        Direction::Horizontal => group()
            .absolute()
            .anchor(Anchor::BottomLeft)
            .height(THUMB_WIDTH)
            .width(thumb_size.clone())
            .y(-THUMB_INSET)
            .x(thumb_pos.clone()),
    };

    base
        .material_opt(thumb_fill)
        .visible(move || max_visible.get() > 0.0 && op_visible.get() > 0.0)
        .on_click_handled(move |_| Propagation::Stop)
        .on_pointer_enter(move |_: &HoverInfo| {
            hover_thumb_enter.set(true);
        })
        .on_pointer_move(move |_: &PointerEvent| {
            hover_thumb_move.set(true);
        })
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
                offset_step.set(if bounce {
                    rubber_band(raw, 0.0, max).clamp(lo, hi)
                } else {
                    raw.clamp(0.0, max)
                });
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

struct WindowTracker {
    direction: Direction,
    offset: Signal<f32>,
    view: ElementRef,
    window: Signal<(usize, usize)>,
    len: usize,
    row_size: f32,
    overscan: usize,
}

const WINDOW_HYSTERESIS: usize = 4;

impl WindowTracker {
    fn resolve(&self) {
        let Some(view) = self.view.rect() else {
            return;
        };
        let view_extent = match self.direction {
            Direction::Vertical => view.height,
            Direction::Horizontal => view.width,
        };
        let off = self.offset.get().max(0.0);
        let need_first = (off / self.row_size).floor() as usize;
        let need_last = ((off + view_extent) / self.row_size).ceil() as usize;
        let (cur_s, cur_e) = self.window.get();
        if cur_e > cur_s
            && need_first >= cur_s.saturating_add(WINDOW_HYSTERESIS).min(self.len)
            && need_last.saturating_add(WINDOW_HYSTERESIS).min(self.len) <= cur_e
        {
            return;
        }
        let start = need_first.saturating_sub(self.overscan).min(self.len);
        let end = (need_last + self.overscan).min(self.len).max(start);
        self.window.set_if_changed((start, end));
    }
}

#[component]
pub fn VirtualScrollView(
    width: Size,
    height: Size,
    len: usize,
    row_height: f32,
    row: RowFn,
    direction: Option<Direction>,
    overscroll: Option<f32>,
    offset: Option<Signal<f32>>,
    handle: Option<ScrollHandle>,
    on_scroll: Option<OnScroll>,
    sticky: Option<Element>,
    scrollbar: Option<Scrollbar>,
    thumb_fill: Option<Material>,
    bounce: Option<bool>,
) -> impl IntoElement {
    let direction = direction.unwrap_or(Direction::Vertical);
    let overscroll = overscroll.unwrap_or(DEFAULT_OVERSCROLL);
    let scrollbar = scrollbar.unwrap_or(Scrollbar::Auto);
    let bounce = bounce.unwrap_or(true);
    let row_size = row_height.max(1.0);

    let owned = use_scroll(0.0);
    let offset_sig = offset
        .or_else(|| handle.clone().map(|h| h.offset))
        .unwrap_or_else(|| owned.offset.clone());
    let max_sig = handle
        .map(|h| h.max)
        .unwrap_or_else(|| owned.max.clone());
    let viewport_ref = use_ref();
    let content_ref = use_ref();
    let thumb_pos = use_signal(0.0f32);
    let thumb_size = use_signal(0.0f32);
    let thumb_travel = use_signal(0.0f32);
    let thumb_op = use_signal(0.0f32);
    let overscan = ((overscroll / row_size).ceil() as usize).max(2);

    let initial_end = match (direction, width, height) {
        (Direction::Vertical, _, Size::Fixed(h)) => ((h / row_size).ceil() as usize + overscan).min(len),
        (Direction::Horizontal, Size::Fixed(w), _) => ((w / row_size).ceil() as usize + overscan).min(len),
        _ => (overscan * 2).min(len),
    };
    let window = use_signal((0usize, initial_end));

    let Some(scope) = current_scope() else {
        no_scope()
    };
    let fling: Rc<Cell<f32>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<f32>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(0.0f32));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let gesture: Rc<ScrollGesture> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<ScrollGesture>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(ScrollGesture {
                    start: Cell::new(0.0),
                    locked_out: Cell::new(false),
                });
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let held: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let soft: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let hovered: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };

    let slot = next_slot(&scope);
    match scope
        .is_stable()
        .then(|| recall_value::<Rc<ScrollClamp>>(&scope, slot))
        .flatten()
    {
        Some(_) => {}
        None => {
            let cell = Rc::new(ScrollClamp {
                direction,
                offset: offset_sig.clone(),
                max: max_sig.clone(),
                fling: Rc::clone(&fling),
                held: Rc::clone(&held),
                bounce,
                viewport: viewport_ref.clone(),
                content: content_ref.clone(),
                thumb_pos: thumb_pos.clone(),
                thumb_size: thumb_size.clone(),
                thumb_travel: thumb_travel.clone(),
                thumb_op: thumb_op.clone(),
                idle: Cell::new(f32::INFINITY),
                thumbs: scrollbar != Scrollbar::Hidden,
                on_scroll,
                last_emitted: Cell::new(f32::NAN),
                last_max: Cell::new(0.0),
                last_op: Cell::new(0.0),
                reveal: Cell::new(0.0),
                primed: Cell::new(false),
                was_beyond: Cell::new(false),
                soft: Rc::clone(&soft),
                hovered: Rc::clone(&hovered),
            });
            if scope.is_stable() {
                remember_value(&scope, slot, &cell);
            }
            offset_sig.attach(Box::new(ScrollPump {
                cell: Rc::downgrade(&cell),
            }));
            scope
                .runtime()
                .borrow_mut()
                .register_post_layout(move |dt| cell.resolve(dt));
        }
    }

    {
        let slot = next_slot(&scope);
        match scope
            .is_stable()
            .then(|| recall_value::<Rc<WindowTracker>>(&scope, slot))
            .flatten()
        {
            Some(_) => {}
            None => {
                let tracker = Rc::new(WindowTracker {
                    direction,
                    offset: offset_sig.clone(),
                    view: viewport_ref.clone(),
                    window: window.clone(),
                    len,
                    row_size,
                    overscan,
                });
                if scope.is_stable() {
                    remember_value(&scope, slot, &tracker);
                }
                scope
                    .runtime()
                    .borrow_mut()
                    .register_post_layout(move |_dt| tracker.resolve());
            }
        }
    }

    let total_extent = len as f32 * row_size;
    let window_sig = window.clone();

    let mut content_group = group()
        .absolute()
        .isolate_children(true)
        .direction(direction)
        .scroll_offset(offset_sig.clone())
        .bind_ref(content_ref)
        .children_of(move || {
            let (s, e) = window_sig.get();
            (s..e)
                .map(|i| {
                    let build = || match direction {
                        Direction::Vertical => group()
                            .key(format!("vrow-{i}"))
                            .absolute()
                            .width(Size::Percent(1.0))
                            .height(row_size)
                            .margin(Margin::top(i as f32 * row_size))
                            .children([(row.0)(i)])
                            .into_element(),
                        Direction::Horizontal => group()
                            .key(format!("vrow-{i}"))
                            .absolute()
                            .height(Size::Percent(1.0))
                            .width(row_size)
                            .margin(Margin::left(i as f32 * row_size))
                            .children([(row.0)(i)])
                            .into_element(),
                    };
                    match crate::reactive::Cx::current() {
                        Some(cx) => cx.enter_keyed(format!("vrow-{i}"), build),
                        None => build(),
                    }
                })
                .collect()
        });

    content_group = match direction {
        Direction::Vertical => content_group.width(Size::Percent(1.0)).height(total_extent),
        Direction::Horizontal => content_group.height(Size::Percent(1.0)).width(total_extent),
    };

    let viewport = scroll_viewport_group(
        width,
        height,
        direction,
        overscroll,
        viewport_ref.clone(),
        &offset_sig,
        &max_sig,
        &fling,
        &held,
        &gesture,
        &viewport_ref,
        &thumb_pos,
        &thumb_size,
        bounce,
        &soft,
        &hovered,
    )
    .children([content_group.into_element()]);

    let viewport = match sticky {
        Some(pinned) => viewport
            .children([group().absolute().children([pinned]).into_element()]),
        None => viewport,
    };
    if scrollbar == Scrollbar::Hidden {
        return viewport.into_element();
    }
    viewport
        .children([scroll_thumb(
            direction,
            &offset_sig,
            &max_sig,
            &fling,
            &held,
            thumb_fill,
            &thumb_pos,
            &thumb_size,
            &thumb_travel,
            &thumb_op,
            bounce,
            &soft,
            &hovered,
        )])
        .into_element()
}
