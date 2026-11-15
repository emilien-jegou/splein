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
// - Desired differs from last frame by any amount: moving — displayed
//   follows exactly. Deliberately no deadband, no glue: holding slow
//   motion to the integer is what quantized text_smooth's drift into
//   pixel steps. Micro-oscillation just tracks exactly at sub-quantum
//   amplitude (invisible) instead of buzzing, and there is no snap-back
//   cycle for a layout loop to sustain.
// - Desired bit-identical to last frame: truly still. Displayed glides
//   toward `round(desired)` at 1px/frame and latches. Sleep is immediate —
//   no calm window, no threshold — so settling always completes on the
//   first still frame and can never strand mid-diary when compilation
//   pauses.
// - The latch only engages on still frames, so oscillation can never
//   flicker it: crossing a pixel boundary while moving just tracks
//   through, and the final rest snaps home at most half a pixel, masked
//   by the motion that caused it.

/// Glide rate toward the rounded baseline once still (layout px per frame).
const GLIDE_PER_FRAME: f32 = 1.0;

/// Desired/displayed Y record for one text node. Lives on the node, so it
/// survives reshapes and rebuilds that would reallocate a layout.
#[derive(Clone, Debug)]
pub struct TextPlacement {
    desired_y: f32,
    displayed_y: f32,
    latched: Option<f32>,
    live: bool,
}

impl Default for TextPlacement {
    fn default() -> Self {
        Self {
            desired_y: 0.0,
            displayed_y: 0.0,
            latched: None,
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
            self.latched = Some(snapped);
            self.live = true;
            return snapped - desired;
        }
        if !self.live {
            let snapped = desired.round();
            self.desired_y = desired;
            self.displayed_y = snapped;
            self.latched = Some(snapped);
            self.live = true;
            return snapped - desired;
        }
        if desired != self.desired_y {
            // Moving (any change at all): follow exactly, drop the latch.
            self.desired_y = desired;
            self.displayed_y = desired;
            self.latched = None;
            return 0.0;
        }
        // Bit-identical desired: truly still. Glide home, latch on arrival.
        let target = desired.round();
        let d = target - self.displayed_y;
        if d == 0.0 {
            self.latched = Some(target);
        } else if d.abs() <= GLIDE_PER_FRAME {
            self.displayed_y = target;
            self.latched = Some(target);
        } else {
            self.displayed_y += d.clamp(-GLIDE_PER_FRAME, GLIDE_PER_FRAME);
            self.latched = None;
        }
        self.displayed_y - desired
    }

    /// Whether the run sits exactly on its rounded baseline. Only settled
    /// runs may replay from the chunk cache: anything else must re-emit so
    /// observation runs on live positions.
    pub fn is_settled(&self) -> bool {
        self.live && self.latched.is_some_and(|l| l == self.displayed_y)
    }

    /// Whether the run still needs observation to reach its baseline.
    /// Pure read (no state change) for the idle backstop.
    pub fn needs_settle(&self) -> bool {
        self.live && !self.is_settled()
    }
}
