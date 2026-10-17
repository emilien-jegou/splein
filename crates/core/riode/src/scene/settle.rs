// Single responsibility: Per-node desired/displayed rect placement.
//
// Generalizes the text settle pattern to all geometry: x, y, width, height.
//
// A node distinguishes two rects:
// - *desired*: where layout put the box this frame (the engine's answer),
// - *displayed*: where the box actually paints (the renderer's answer).
//
// Behavior mirrors TextPlacement:
// - First sight: displayed snaps to round(desired) — crisp debut, no subpixel first paint.
// - Desired differs from last frame: moving — displayed follows exactly (smooth subpixel motion).
// - Desired bit-identical: truly still — displayed glides toward round(desired) at 1px/frame,
//   latches on arrival.
//
// The latch only engages on still frames, so oscillation never flicks it.

use crate::foundation::ResolvedRect;

const GLIDE_PER_FRAME: f32 = 1.0;

#[derive(Clone, Debug)]
pub struct RectSettle {
    pub desired: ResolvedRect,
    pub displayed: ResolvedRect,
    pub latched: Option<ResolvedRect>,
    pub live: bool,
}

impl Default for RectSettle {
    fn default() -> Self {
        Self {
            desired: ResolvedRect::ZERO,
            displayed: ResolvedRect::ZERO,
            latched: None,
            live: false,
        }
    }
}

fn snap_rect(r: ResolvedRect) -> ResolvedRect {
    ResolvedRect::new(r.x.round(), r.y.round(), r.width.round(), r.height.round())
}

impl RectSettle {
    /// Observes one frame's desired rect, returning the settled paint rect.
    pub fn observe(&mut self, desired: ResolvedRect) -> ResolvedRect {
        if !desired.is_finite() {
            return self.displayed;
        }
        if !self.live {
            let snapped = snap_rect(desired);
            self.desired = desired;
            self.displayed = snapped;
            self.latched = Some(snapped);
            self.live = true;
            return snapped;
        }
        if desired != self.desired {
            self.desired = desired;
            self.displayed = desired;
            self.latched = None;
            return desired;
        }
        let target = snap_rect(desired);
        let d = ResolvedRect::new(
            target.x - self.displayed.x,
            target.y - self.displayed.y,
            target.width - self.displayed.width,
            target.height - self.displayed.height,
        );
        if d.x == 0.0 && d.y == 0.0 && d.width == 0.0 && d.height == 0.0 {
            self.latched = Some(target);
        } else {
            let move_towards = |val: f32, delta: f32| -> f32 {
                if delta.abs() <= GLIDE_PER_FRAME {
                    val + delta
                } else {
                    val + delta.clamp(-GLIDE_PER_FRAME, GLIDE_PER_FRAME)
                }
            };
            self.displayed = ResolvedRect::new(
                move_towards(self.displayed.x, d.x),
                move_towards(self.displayed.y, d.y),
                move_towards(self.displayed.width, d.width),
                move_towards(self.displayed.height, d.height),
            );
            let remaining = ResolvedRect::new(
                target.x - self.displayed.x,
                target.y - self.displayed.y,
                target.width - self.displayed.width,
                target.height - self.displayed.height,
            );
            if remaining.x == 0.0 && remaining.y == 0.0 && remaining.width == 0.0 && remaining.height == 0.0
            {
                self.latched = Some(target);
            } else {
                self.latched = None;
            }
        }
        self.displayed
    }

    pub fn is_settled(&self) -> bool {
        self.live && self.latched.is_some_and(|l| l == self.displayed)
    }

    pub fn needs_settle(&self) -> bool {
        self.live && !self.is_settled()
    }
}
