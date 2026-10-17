// Single responsibility: Y-only settle tracker — subpixel while moving, integer when resting.
//
// Loosely coupled by design: position-in / position-out, no dependency on
// `anim/` or any notion of "is animating". Each text run (keyed by layout
// allocation identity) keeps a short diary of recent Y positions; only when
// the total wandering inside the window stays below epsilon is the run
// considered resting and rounded to the nearest whole pixel. A nudge every
// 2nd/3rd/4th frame still shows up as wandering inside the window, so slow
// drift never false-settles into a buzz.

use std::collections::HashMap;

/// Frames of Y history consulted before declaring rest.
const WINDOW: usize = 6;

/// Total wandering inside the window below this counts as still (layout px).
const EXCURSION_EPS: f32 = 1.0 / 16.0;

/// Per-run diary entry.
#[derive(Clone, Debug)]
struct Entry {
    /// Recent raw Y positions (oldest..newest, up to WINDOW).
    history: [f32; WINDOW],
    /// How many slots of `history` are filled.
    len: usize,
    /// Write cursor into `history`.
    cursor: usize,
    /// Whether the run has settled (and at what snapped Y).
    anchor: Option<f32>,
}

impl Entry {
    fn new() -> Self {
        Self {
            history: [0.0; WINDOW],
            len: 0,
            cursor: 0,
            anchor: None,
        }
    }

    fn push(&mut self, y: f32) {
        self.history[self.cursor] = y;
        self.cursor = (self.cursor + 1) % WINDOW;
        self.len = (self.len + 1).min(WINDOW);
    }

    fn excursion(&self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for v in self.history.iter().take(self.len) {
            min = min.min(*v);
            max = max.max(*v);
        }
        max - min
    }
}

/// Tracks per-run Y rest state. Keyed by caller-provided run identity
/// (layout allocation pointer); bounded by caller eviction via the
/// line-cache lifetime — entries for dropped layouts are pruned lazily.
#[derive(Debug, Default)]
pub struct YSettleTracker {
    entries: HashMap<usize, Entry>,
}

impl YSettleTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves the Y to emit for `key` given the raw Y this frame.
    ///
    /// Moving (window excursion above epsilon, or window not yet full):
    /// returns `y` unchanged. Resting: returns the latched `round(y)`,
    /// sticky until excursion breaks it again.
    pub fn resolve(&mut self, key: usize, y: f32) -> f32 {
        if !y.is_finite() {
            return y;
        }
        // Bound growth: layouts come and go; drop stale entries lazily.
        if self.entries.len() > 8192 {
            self.entries.clear();
        }
        let entry = self.entries.entry(key).or_insert_with(Entry::new);
        entry.push(y);
        if entry.len < WINDOW || entry.excursion() > EXCURSION_EPS {
            entry.anchor = None;
            return y;
        }
        match entry.anchor {
            Some(a) => a,
            None => {
                let snapped = y.round();
                entry.anchor = Some(snapped);
                snapped
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_passes_through_until_window_calms() {
        let mut t = YSettleTracker::new();
        // Drift keeps passing through.
        for i in 0..WINDOW {
            let y = 100.0 + i as f32 * 0.5;
            assert_eq!(t.resolve(1, y), y);
        }
        // Then stillness settles to round.
        let mut last = 0.0;
        for _ in 0..WINDOW {
            last = t.resolve(1, 100.4);
        }
        assert_eq!(last, 100.0);
        // A nudge above epsilon wakes back up.
        assert_eq!(t.resolve(1, 100.4 + EXCURSION_EPS * 2.0), 100.4 + EXCURSION_EPS * 2.0);
    }

    #[test]
    fn slow_nudge_every_fourth_frame_never_settles() {
        let mut t = YSettleTracker::new();
        // 3 still frames, 1 nudge, repeating: excursion inside any
        // 6-frame window always exceeds epsilon → never snaps.
        let mut snapped = false;
        for i in 0..24 {
            let y = if i % 4 == 3 { 100.4 } else { 100.0 };
            let out = t.resolve(7, y);
            if out != y {
                snapped = true;
            }
        }
        assert!(!snapped, "slow stepping motion must not settle");
    }

    #[test]
    fn non_finite_passes_through() {
        let mut t = YSettleTracker::new();
        assert!(t.resolve(3, f32::NAN).is_nan());
    }
}
