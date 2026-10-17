// Single responsibility: Frame delta source with a deterministic override for headless stepping.

use std::time::Instant;

/// Longest measured delta accepted, so a stall cannot teleport an animation.
const MAX_DELTA_SECS: f32 = 0.1;

/// The quantum animation is integrated in, independent of how fast frames arrive.
///
/// Springs and tweens are integrated explicitly, so the result depends on the step size, not
/// only on the total elapsed time. Advancing by whatever the frame happened to measure means a
/// transition lands in a different place at 30fps than at 144fps, and the discrepancy is
/// invisible in a screenshot and obvious in a side-by-side. Integrating on a fixed grid makes
/// the trajectory a function of elapsed time alone.
///
/// 120Hz is comfortably above any display and low enough that the extra passes are cheap.
pub const FIXED_STEP_SECS: f32 = 1.0 / 120.0;

/// Most quanta one frame may run.
///
/// Only a guard against an absurd value reaching the integrators; the real bound on a live
/// frame is [`MAX_DELTA_SECS`], which already caps a *measured* delta at exactly twelve
/// quanta. A forced delta is honoured in full, because forcing time is an explicit instruction
/// -- a headless step that asked to advance 400ms means 400ms, and quietly giving it 100ms
/// would make every such test assert against a different simulation than the one it described.
const MAX_STEPS_PER_FRAME: u32 = 3600;

/// Frame delta source yielding either a forced or a measured delta in seconds.
pub struct Clock {
    previous: Instant,
    forced: Option<f32>,
    last: f32,
    /// Wall time since the previous tick: unclamped and unscaled.
    ///
    /// Animation clamps (`MAX_DELTA_SECS`) and scales its deltas, which is
    /// correct for springs but fatal for gesture timeouts: after an idle
    /// stretch the next frame reports at most 0.1s no matter how long the
    /// user was actually away, so anything measured against `last` (like the
    /// double-click burst) can never expire in an on-demand frame loop.
    wall: f32,
    scale: f32,
    /// Total animation time simulated so far, in seconds.
    ///
    /// Counts only whole quanta actually integrated, so it is the honest answer to "how far
    /// has animation advanced", and the only frame-rate-independent measure of that. Useful for
    /// diagnostics, and what lets a test line two frame rates up on the same simulated instant
    /// instead of on the same frame count -- which is not the same thing, because the frame a
    /// transition adopts its target on is not a frame it advances.
    simulated: f32,
    /// Whole quanta explicitly asked for, which run even while the clock is stopped.
    ///
    /// Freezing stops time completely, so without this there is no way to walk a transition one
    /// frame at a time -- the frozen frame is the only one you can ever see. Forcing a delta
    /// does not help either, because a forced delta is scaled like any other and a scale of
    /// zero is zero however hard it is asked.
    step_requests: u32,
    /// Time banked but not yet spent on a whole quantum.
    ///
    /// Frames arrive at whatever rate the display and the compositor agree on, while
    /// animation advances on [`FIXED_STEP_SECS`]. The remainder of each frame's delta is kept
    /// here so no time is lost to rounding, which is what keeps the total honest across
    /// thousands of frames.
    accumulated: f32,
}

impl Clock {
    /// Creates a clock measuring its first delta from now.
    pub fn new() -> Self {
        Self {
            previous: Instant::now(),
            forced: None,
            last: 0.0,
            wall: 0.0,
            scale: 1.0,
            simulated: 0.0,
            step_requests: 0,
            accumulated: 0.0,
        }
    }

    /// Forces the next delta, so headless tests can step time deterministically.
    pub fn force(&mut self, dt: f32) {
        self.forced = Some(dt);
    }

    /// Sets the rate time runs at. `0.0` freezes the engine; `0.1` is slow motion.
    ///
    /// Scaling happens where the delta is produced, so it applies to measured time as well
    /// as forced time and therefore reaches every time-driven system at once: springs,
    /// tweens, gesture velocity, and the reactive tickers behind them.
    pub fn set_time_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.0);
    }

    /// The current time scale.
    #[inline(always)]
    pub fn time_scale(&self) -> f32 {
        self.scale
    }

    /// Consumes the next delta in seconds, already scaled.
    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let measured = (now - self.previous).as_secs_f32();
        self.previous = now;
        // Forced deltas are honoured in full (headless steps mean what they
        // say); measured ones clamp for animation stability.
        let (raw, dt) = match self.forced.take() {
            Some(dt) => {
                let dt = dt.max(0.0);
                (dt, dt)
            }
            None => (measured, measured.min(MAX_DELTA_SECS)),
        };
        // Wall time is the raw gap: no clamp (idle stretches count in full)
        // and no scale (a double-click is 0.4 real seconds even in slow motion).
        self.wall = raw;
        let scaled = dt * self.scale;
        self.last = scaled;
        scaled
    }

    /// How many whole [`FIXED_STEP_SECS`] quanta to integrate this frame, and the size of each.
    ///
    /// Banks the scaled delta produced by [`Self::tick`] and spends it in whole quanta, so a
    /// 30fps frame runs four 120Hz steps and a 144fps frame runs one every few frames. Both
    /// cover the same elapsed time in the same increments, which is the property that makes a
    /// transition land in the same place regardless of frame rate.
    ///
    /// Returns zero steps while time is frozen, so pausing costs nothing and a partially
    /// banked remainder resumes exactly where it stopped rather than being discarded.
    pub fn animation_steps(&mut self) -> (u32, f32) {
        if self.scale <= 0.0 {
            // A requested step is honoured even here: it is the only way to advance a stopped
            // clock by exactly one frame, which is what makes a frozen transition inspectable.
            if self.step_requests > 0 {
                self.step_requests -= 1;
                self.simulated += FIXED_STEP_SECS;
                return (1, FIXED_STEP_SECS);
            }
            // Freeze rather than drain: a paused engine must not bank time it will later
            // replay in one burst, which would fast-forward every animation on resume.
            self.accumulated = 0.0;
            return (0, 0.0);
        }
        // While time is already running a step would be redundant, and carrying it would mean
        // freezing and then thawing jumped forward by a frame nobody asked for.
        self.step_requests = 0;
        self.accumulated += self.last;
        if self.accumulated < FIXED_STEP_SECS {
            return (0, 0.0);
        }
        let steps = ((self.accumulated / FIXED_STEP_SECS).floor() as u32).min(MAX_STEPS_PER_FRAME);
        self.accumulated -= steps as f32 * FIXED_STEP_SECS;
        if steps == MAX_STEPS_PER_FRAME {
            // The clamp engaged, so the bank holds more time than this frame may spend. Drop
            // it: replaying a stall would take longer than real time and never catch up.
            self.accumulated = 0.0;
        }
        self.simulated += steps as f32 * FIXED_STEP_SECS;
        (steps, FIXED_STEP_SECS)
    }

    /// Drops the pending animation delta and banked time.
    ///
    /// Called on frames where no ticker wants animation (all settled): the
    /// measured gap then covers idle time owed to nobody. Both the stored
    /// remainder and the current tick's delta are dropped — dropping only
    /// the remainder is useless, since `animation_steps` would immediately
    /// re-bank the just-measured gap (up to `MAX_DELTA_SECS`) into the next
    /// retarget (a fresh 180ms fade starting 100ms in). Slow-frame catch-up
    /// during live animation is unaffected — this only runs when nothing was
    /// animating. Same principle as the frozen clock zeroing its
    /// accumulator on stop. Note `wall` is kept: gesture timeouts must
    /// still observe real idle stretches.
    pub fn discard_banked(&mut self) {
        self.last = 0.0;
        self.accumulated = 0.0;
    }

    /// Asks for exactly one quantum to run on the next frame, whatever the time scale.
    ///
    /// Requests do not queue up behind each other beyond the frame boundary, so pressing step
    /// faster than frames arrive simply runs one quantum per frame.
    pub fn request_step(&mut self) {
        self.step_requests = self.step_requests.saturating_add(1);
    }

    /// Whether a step is waiting to run, for diagnostics.
    pub fn step_pending(&self) -> bool {
        self.step_requests > 0
    }

    /// Total animation time simulated so far, in seconds.
    pub fn simulated(&self) -> f32 {
        self.simulated
    }

    /// Time banked toward the next quantum, for diagnostics.
    pub fn accumulated(&self) -> f32 {
        self.accumulated
    }

    /// The delta most recently produced by [`Self::tick`], after scaling.
    ///
    /// Lets anything that must move on the same timeline as the frame advance against the
    /// frame's own delta instead of keeping a second, slightly different notion of time.
    #[inline(always)]
    pub fn last_delta(&self) -> f32 {
        self.last
    }

    /// Wall time since the previous tick: unclamped and unscaled.
    ///
    /// What gesture timeouts (click bursts) advance against. Unlike
    /// [`Self::last_delta`], an idle stretch reports in full here, so a burst
    /// still expires when frames only run on demand.
    #[inline(always)]
    pub fn wall_delta(&self) -> f32 {
        self.wall
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Advances `frames` frames of `dt` each, returning the quanta spent and the total.
    fn drain(clock: &mut Clock, frames: u32, dt: f32) -> (u32, f32) {
        let mut steps = 0;
        let mut spent = 0.0;
        for _ in 0..frames {
            clock.force(dt);
            clock.tick();
            let (n, step) = clock.animation_steps();
            steps += n;
            spent += n as f32 * step;
        }
        (steps, spent)
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_whole_number_of_quarters_spends_exactly_that_much_time() {
        let mut clock = Clock::new();
        let (steps, spent) = drain(&mut clock, 120, 1.0 / 120.0);
        assert_eq!(steps, 120, "a 120Hz frame must run exactly one quantum");
        assert!(
            (spent - 1.0).abs() < 1e-4,
            "a second of frames must advance a second of animation, got {spent}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_slow_frame_runs_the_same_quanta_as_many_fast_ones() {
        // The whole point of the grid: elapsed time, not frame rate, decides the trajectory.
        let mut fast = Clock::new();
        let (fast_steps, fast_time) = drain(&mut fast, 48, 1.0 / 120.0);
        let mut slow = Clock::new();
        let (slow_steps, slow_time) = drain(&mut slow, 12, 1.0 / 30.0);

        assert_eq!(fast_steps, slow_steps, "equal time must cost equal quanta");
        assert!(
            (fast_time - slow_time).abs() < 1e-4,
            "equal frames of time must advance equal animation: {fast_time} vs {slow_time}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_frame_too_short_for_a_quantum_spends_nothing_and_keeps_the_remainder() {
        let mut clock = Clock::new();
        let (steps, _) = drain(&mut clock, 6, 1.0 / 1000.0);
        assert_eq!(steps, 0, "a 1ms frame cannot pay for an 8.3ms quantum");

        let (steps, spent) = drain(&mut clock, 1, 0.0);
        assert_eq!(steps, 0);
        assert!((spent - 0.0).abs() < 1e-9);
        assert!(
            clock.accumulated() > 0.0,
            "the partial quantum must be banked, not discarded, or a rate that never lands \\
             exactly on the quantum would never animate at all"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn banking_survives_a_rate_that_never_divides_the_quantum() {
        // 60Hz divides 120Hz evenly, which makes it the one rate that would hide a lost
        // remainder. 90fps does not, and must still advance the right total.
        let mut clock = Clock::new();
        let (_, spent) = drain(&mut clock, 90, 1.0 / 90.0);
        // One quantum of lag is inherent: a frame only spends whole quanta, so the partial
        // one waits. What matters is that it is *banked* rather than dropped, which the
        // companion test on conservation covers directly.
        assert!(
            (spent - 1.0).abs() <= FIXED_STEP_SECS + 1e-4,
            "a second at 90fps must advance a second of animation, got {spent}"
        );
        assert!(
            clock.accumulated() > 0.0,
            "the unfinished quantum must be carried, not discarded"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_frozen_clock_spends_nothing_and_banks_nothing() {
        let mut clock = Clock::new();
        clock.force(1.0 / 60.0);
        clock.tick();
        let (steps, _) = clock.animation_steps();
        assert_eq!(steps, 2);

        clock.set_time_scale(0.0);
        clock.force(1.0 / 60.0);
        clock.tick();
        let (steps, _) = clock.animation_steps();
        assert_eq!(steps, 0, "a frozen clock must not advance anything");
        assert_eq!(
            clock.accumulated(),
            0.0,
            "time banked while frozen would be replayed in one burst on resume"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_slow_clock_spends_its_time_at_the_slow_rate() {
        let mut clock = Clock::new();
        clock.set_time_scale(0.1);
        // Sixty frames at 60fps is one second of real time, a tenth of which is the animation.
        let (steps, spent) = drain(&mut clock, 60, 1.0 / 60.0);
        assert!(
            (spent - 0.1).abs() < 1e-3,
            "a tenth-speed clock must advance a tenth as far, got {spent}"
        );
        assert_eq!(steps, 12, "which is twelve quanta, not sixty");
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_forced_delta_is_honoured_in_full() {
        // Headless steps state exactly how much time they mean. Silently truncating that to
        // the per-frame clamp makes a test assert against a different simulation than the one
        // it described, which is worse than a slow test.
        let mut clock = Clock::new();
        clock.force(2.0);
        clock.tick();
        let (first, step) = clock.animation_steps();
        assert!(
            first > 200,
            "a forced two seconds must not be truncated to a frame's worth, got {first} quanta"
        );
        assert!(
            clock.accumulated() > 0.0,
            "the leftover must be banked rather than thrown away"
        );

        // Once more time arrives, the bank must recover what the first frame could not spend in
        // whole quanta. This is the property that matters: no time is ever lost, so a long
        // animation cannot drift away from its duration.
        //
        // The next frame has to carry *some* time for this to work. A remainder a hair under a
        // quantum can never complete on its own, which is harmless in practice -- real frames
        // keep delivering deltas -- but it does mean a clock fed exactly zero forever would sit
        // on that remainder instead of advancing.
        clock.force(FIXED_STEP_SECS);
        clock.tick();
        let (second, _) = clock.animation_steps();
        let total = (first + second) as f32 * step;
        assert!(
            (total - 2.0).abs() < 1e-3,
            "the two frames must together simulate the two seconds asked for, got {total}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_stepped_frame_advances_exactly_one_quantum() {
        let mut clock = Clock::new();
        clock.set_time_scale(0.0);
        clock.request_step();
        clock.force(1.0 / 60.0);
        clock.tick();
        let (steps, step) = clock.animation_steps();

        assert_eq!(
            steps, 1,
            "one request must advance one quantum, not a frame's worth"
        );
        assert!((step - FIXED_STEP_SECS).abs() < 1e-9);
        assert!(
            !clock.step_pending(),
            "the request must be consumed, or steps would queue up forever"
        );
        assert!(
            (clock.simulated() - FIXED_STEP_SECS).abs() < 1e-9,
            "a step is simulated time and must be accounted as such"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn steps_are_spent_one_frame_at_a_time_and_do_not_accumulate() {
        let mut clock = Clock::new();
        clock.set_time_scale(0.0);
        clock.request_step();
        clock.request_step();
        clock.request_step();

        let mut total = 0u32;
        for _ in 0..3 {
            clock.force(1.0 / 60.0);
            clock.tick();
            total += clock.animation_steps().0;
        }
        assert_eq!(total, 3, "three requests must take three frames, not one");
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_frozen_clock_with_nothing_requested_stays_put() {
        let mut clock = Clock::new();
        clock.set_time_scale(0.0);
        for _ in 0..10 {
            clock.force(1.0 / 60.0);
            clock.tick();
            assert_eq!(clock.animation_steps().0, 0);
        }
        assert_eq!(clock.simulated(), 0.0);
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_request_made_while_running_is_dropped_rather_than_saved_for_the_next_freeze() {
        // Otherwise stepping once mid-animation would jump a frame when time stopped, which is
        // the opposite of what the key looks like it does.
        let mut clock = Clock::new();
        clock.request_step();
        clock.force(1.0 / 60.0);
        clock.tick();
        let (steps, _) = clock.animation_steps();
        assert_eq!(steps, 2, "two quanta from the frame itself");
        assert!(!clock.step_pending());

        clock.set_time_scale(0.0);
        clock.force(1.0 / 60.0);
        clock.tick();
        assert_eq!(
            clock.animation_steps().0,
            0,
            "the dropped request must not reappear as a surprise step after freezing"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn scaling_applies_to_measured_time_as_well_as_forced() {
        let mut clock = Clock::new();
        clock.set_time_scale(0.5);
        clock.force(0.1);
        let dt = clock.tick();
        assert!(
            (dt - 0.05).abs() < 1e-6,
            "scaling must happen where the delta is produced, got {dt}"
        );
    }
}
