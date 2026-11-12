// Single responsibility: Post-layout scroll physics, fling decay, spring interpolation, and thumb animation.

use std::cell::Cell;
use std::rc::{Rc, Weak};
use crate::dsl::element_ref::ElementRef;
use crate::foundation::Direction;
use crate::reactive::{Signal, Ticker};
use super::*;

#[derive(Clone)]
pub(crate) struct ScrollGesture {
    pub start: Cell<f32>,
    pub locked_out: Cell<bool>,
}

pub(crate) struct ScrollPump {
    pub cell: Weak<ScrollClamp>,
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

pub(crate) struct ScrollClamp {
    pub direction: Direction,
    pub offset: Signal<f32>,
    pub max: Signal<f32>,
    pub fling: Rc<Cell<f32>>,
    pub held: Rc<Cell<bool>>,
    pub bounce: bool,
    pub viewport: ElementRef,
    pub content: ElementRef,
    pub thumb_pos: Signal<f32>,
    pub thumb_size: Signal<f32>,
    pub thumb_travel: Signal<f32>,
    pub thumb_op: Signal<f32>,
    pub last_max: Cell<f32>,
    pub last_op: Cell<f32>,
    pub reveal: Cell<f32>,
    pub idle: Cell<f32>,
    pub hovered: Rc<Cell<bool>>,
    pub thumbs: bool,
    pub on_scroll: Option<OnScroll>,
    pub last_emitted: Cell<f32>,
    pub primed: Cell<bool>,
    pub was_beyond: Cell<bool>,
    pub soft: Rc<Cell<bool>>,
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
                    self.fling.set(if decayed.abs() < FLING_STOP { 0.0 } else { decayed });
                }
            }
        }
        let settled = self.offset.get();
        self.was_beyond.set(settled < 0.0 || settled > max);
    }

    pub fn resolve(&self, dt: f32) {
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
                    self.fling.set(if decayed.abs() < FLING_STOP { 0.0 } else { decayed });
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
            self.update_thumbs(view_extent, content_extent, max, current, dt);
        }
    }

    fn update_thumbs(&self, view_extent: f32, content_extent: f32, max: f32, current: f32, dt: f32) {
        let lane = (view_extent - 2.0 * THUMB_EDGE).max(0.0);
        let thumb_size = if max <= 0.0 {
            0.0
        } else {
            (view_extent * view_extent / content_extent).clamp(THUMB_MIN, view_extent).min(lane)
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
