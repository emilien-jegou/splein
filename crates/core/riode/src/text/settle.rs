// Single responsibility: Per-text-node desired/displayed Y placement.
//
// A text node distinguishes two positions:
// - *desired*: where layout put the run this frame (the engine's answer),
// - *displayed*: where the run actually paints (the font's answer).
//
// Self-contained by design: position-in / adjustment-out, no dependency on
// `anim/` or any notion of "is animating". Motion is read off the desired
// positions themselves, so transitions, springs, overshoots, retargets,
// scrolls, and resizes all read correctly without the text domain knowing
// any of those things exist.
//
// Behavior:
// - First sight of a node: displayed snaps to `round(desired)` — correct
//   baseline on the very first paint, never a subpixel debut.
// - Desired moves more than epsilon from displayed: moving — displayed
//   follows exactly (smooth motion).
// - Otherwise calm: after a still window, displayed glides toward
//   `round(desired)` at 1px/frame and latches (resize corrections land
//   instead of popping late).
// - Slow drift can never buzz: any divergence above epsilon re-arms motion,
//   so a creeping desired always drags displayed along.

/// Frames of stillness before a resting run starts gliding home.
const CALM_WINDOW: u8 = 6;

/// Frame-to-frame (and displayed-vs-desired) divergence above this counts
/// as motion (layout px).
const MOTION_EPS: f32 = 1.0 / 16.0;

/// Glide rate toward the rounded baseline once calm (layout px per frame).
const GLIDE_PER_FRAME: f32 = 1.0;

/// Latch distance: close enough to call it landed (layout px).
const LATCH_EPS: f32 = 1.0 / 16.0;

/// Desired/displayed Y record for one text node. Lives on the node, so it
/// survives reshapes and rebuilds that would reallocate a layout.
#[derive(Clone, Debug)]
pub struct TextPlacement {
    desired_y: f32,
    displayed_y: f32,
    calm_frames: u8,
    live: bool,
}

impl Default for TextPlacement {
    fn default() -> Self {
        Self {
            desired_y: 0.0,
            displayed_y: 0.0,
            calm_frames: 0,
            live: false,
        }
    }
}

impl TextPlacement {
    /// Observes one frame's desired absolute Y, returning the adjustment to
    /// add to it (`displayed - desired`). When `snap_now` (the `B`-key
    /// global snap), jumps straight to the rounded baseline.
    pub fn observe(&mut self, desired: f32, snap_now: bool) -> f32 {
        if !desired.is_finite() {
            return 0.0;
        }
        if snap_now {
            let snapped = desired.round();
            self.desired_y = desired;
            self.displayed_y = snapped;
            self.calm_frames = CALM_WINDOW;
            self.live = true;
            return snapped - desired;
        }
        if !self.live {
            let snapped = desired.round();
            self.desired_y = desired;
            self.displayed_y = snapped;
            self.calm_frames = CALM_WINDOW;
            self.live = true;
            return snapped - desired;
        }
        // Any divergence above epsilon — a jump, a step, or accumulated
        // slow drift — is motion: follow exactly.
        if (desired - self.displayed_y).abs() > MOTION_EPS {
            self.desired_y = desired;
            self.displayed_y = desired;
            self.calm_frames = 0;
            return 0.0;
        }
        self.desired_y = desired;
        self.calm_frames = self.calm_frames.saturating_add(1);
        if self.calm_frames < CALM_WINDOW {
            return self.displayed_y - desired;
        }
        // Calm: glide home at a bounded rate, latch when close.
        let target = desired.round();
        let d = target - self.displayed_y;
        if d.abs() <= LATCH_EPS {
            self.displayed_y = target;
        } else {
            self.displayed_y += d.clamp(-GLIDE_PER_FRAME, GLIDE_PER_FRAME);
        }
        self.displayed_y - desired
    }
}
