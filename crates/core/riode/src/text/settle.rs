// Single responsibility: Y-only rest detector — subpixel while moving, settled when resting.
//
// Loosely coupled by design: position-in / bool-out, no dependency on
// `anim/` or any notion of "is animating". Each text run (keyed by layout
// allocation identity) keeps a short diary of recent Y positions; only when
// the total wandering inside the window stays below epsilon is the run
// considered resting. A nudge every 2nd/3rd/4th frame still shows up as
// wandering inside the window, so slow drift never false-settles into a buzz.
//
// This module only *decides*; the caller owns the *rounding* (in whatever
// space it renders — layout px in core compile, device px in engines).

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
    /// Latched rest state. `None` while moving.
    settled: bool,
}

impl Entry {
    fn new() -> Self {
        Self {
            history: [0.0; WINDOW],
            len: 0,
            cursor: 0,
            settled: false,
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

    /// Observes one frame's raw Y for `key`, returning whether the run is
    /// currently resting. Moving (window excursion above epsilon, or window
    /// not yet full): returns false. Resting: returns true, sticky until
    /// excursion breaks it again.
    pub fn observe(&mut self, key: usize, y: f32) -> bool {
        if !y.is_finite() {
            return false;
        }
        // Bound growth: layouts come and go; drop stale entries lazily.
        if self.entries.len() > 8192 {
            self.entries.clear();
        }
        let entry = self.entries.entry(key).or_insert_with(Entry::new);
        entry.push(y);
        if entry.len < WINDOW || entry.excursion() > EXCURSION_EPS {
            entry.settled = false;
            return false;
        }
        entry.settled = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_reports_unsettled_until_window_calms() {
        let mut t = YSettleTracker::new();
        // Drift keeps reporting moving.
        for i in 0..WINDOW {
            let y = 100.0 + i as f32 * 0.5;
            assert!(!t.observe(1, y));
        }
        // Then stillness settles.
        let mut settled = false;
        for _ in 0..WINDOW {
            settled = t.observe(1, 100.4);
        }
        assert!(settled);
        // A nudge above epsilon wakes back up.
        assert!(!t.observe(1, 100.4 + EXCURSION_EPS * 2.0));
    }

    #[test]
    fn slow_nudge_every_fourth_frame_never_settles() {
        let mut t = YSettleTracker::new();
        // 3 still frames, 1 nudge, repeating: excursion inside any
        // 6-frame window always exceeds epsilon → never settled.
        for i in 0..24 {
            let y = if i % 4 == 3 { 100.4 } else { 100.0 };
            assert!(!t.observe(7, y), "slow stepping motion must not settle");
        }
    }

    #[test]
    fn non_finite_is_not_settled() {
        let mut t = YSettleTracker::new();
        assert!(!t.observe(3, f32::NAN));
    }
}
