// Single responsibility: Animation advances on elapsed time, not on how many frames arrived.
//
// The engine's clock integrates in fixed quanta. That is only worth having if it actually
// delivers the property it exists for, so these tests measure the same animation at frame
// rates that differ by an order of magnitude and require the same answer.

use aurora::foundation::Color;
use aurora::prelude::*;
use aurora::testing::Harness;

/// A card whose x springs to 120, driven by a plain linear tween so any difference in the
/// result comes from the clock rather than from an integrator.
#[component]
fn Slide() -> impl IntoElement {
    let go = use_signal(false);
    let target = go.map(|g| if g { 120.0 } else { 0.0 });
    let x = Transition::ease(400.ms(), Ease::Linear).on(target);
    let toggle = go.clone();
    let state = go;
    group()
        .width(400.0)
        .height(200.0)
        .fill(Color::WHITE)
        .children([group()
            .key("card")
            .width(40.0)
            .height(40.0)
            .fill(Color::hex(0x1D4ED8))
            .x(x)
            .on_click(move |_| toggle.set(!state.get()))])
}

/// Runs one tween until `simulated_seconds` of animation time has elapsed at `fps`.
///
/// Steps until the *clock* says the instant has arrived rather than counting frames. The frame
/// on which a transition adopts its new target is not a frame it advances, so equal frame counts
/// are not equal simulated times -- and counting frames produced a convincing false failure that
/// looked precisely like the frame-rate dependence this file exists to rule out.
fn tween_position_at(fps: f32, simulated_seconds: f32) -> f32 {
    let mut h = Harness::page(Slide, 400, 200).with_delta(1.0 / fps);
    h.steps(2);

    // Baseline before the press, because the click's own frames are the ones the tween spends
    // starting up. Comparing absolute clock readings instead compares how long each page had
    // been alive, not how far the transition had come.
    let pre = h.engine().clock_debug_simulated();
    h.click("card");

    let mut guard = 0;
    while h.engine().clock_debug_simulated() - pre < simulated_seconds && guard < 20_000 {
        h.step();
        guard += 1;
    }
    h.transform("card").tx
}

#[test]
#[ntest::timeout(30_000)]
fn a_tween_lands_in_the_same_place_at_15fps_as_at_240fps() {
    // 200ms into a 400ms linear tween is exactly halfway.
    let at_15 = tween_position_at(15.0, 0.2);
    let at_60 = tween_position_at(60.0, 0.2);
    let at_240 = tween_position_at(240.0, 0.2);

    for (label, value) in [("60", at_60), ("240", at_240)] {
        assert!(
            (value - at_15).abs() < 1.5,
            "at {label}fps the tween is at {value}, but at 15fps it is at {at_15}: the animation \\
             is following the frame rate instead of the clock"
        );
    }
}

#[test]
#[ntest::timeout(30_000)]
fn a_tween_completes_at_the_same_moment_regardless_of_frame_rate() {
    // The visible symptom of frame-rate dependence is not a wrong intermediate value, it is a
    // transition that takes visibly longer on a slow machine.
    let duration_frames_at = |fps: f32| -> f32 {
        let mut h = Harness::page(Slide, 400, 200).with_delta(1.0 / fps);
        h.steps(2);
        h.click("card");
        let mut n = 0;
        while !h.is_settled() && n < 4000 {
            h.step();
            n += 1;
        }
        n as f32 / fps
    };

    let slow = duration_frames_at(20.0);
    let fast = duration_frames_at(240.0);
    assert!(
        (slow - fast).abs() < 0.06,
        "a 400ms tween took {slow:.3}s at 20fps and {fast:.3}s at 240fps"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_slow_clock_stretches_the_animation_without_changing_its_shape() {
    // Slow motion must be the same animation over more frames, not a different one.
    let mut fast = Harness::page(Slide, 400, 200).with_delta(1.0 / 60.0);
    fast.steps(2);
    fast.click("card");
    fast.steps(24); // 400ms of animation, at the end of the tween

    let mut slow = Harness::page(Slide, 400, 200).with_delta(1.0 / 60.0);
    slow.slow_motion(0.1);
    slow.steps(2);
    slow.click("card");
    slow.steps(240); // same 400ms, one tenth as fast

    assert!(
        (fast.transform("card").tx - slow.transform("card").tx).abs() < 1.5,
        "after the same simulated time the card is at {} when fast and {} when slow",
        fast.transform("card").tx,
        slow.transform("card").tx
    );
    assert!(
        slow.transform("card").tx > 100.0 && fast.transform("card").tx > 100.0,
        "both should have arrived, they are at {} and {}",
        fast.transform("card").tx,
        slow.transform("card").tx
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_frozen_clock_holds_a_transition_mid_flight() {
    let mut h = Harness::page(Slide, 400, 200).with_delta(1.0 / 60.0);
    h.steps(2);
    h.click("card");
    h.steps(6);
    let held = h.transform("card").tx;
    assert!(held > 1.0, "the tween must have started, it is at {held}");

    h.pause();
    for _ in 0..30 {
        h.step();
    }
    assert!(
        (h.transform("card").tx - held).abs() < 1e-3,
        "a frozen clock must hold the animation, it moved from {held} to {}",
        h.transform("card").tx
    );

    h.resume();
    h.steps(30);
    assert!(
        h.transform("card").tx > held + 20.0,
        "resuming must continue rather than restart, it is at {}",
        h.transform("card").tx
    );
}

#[test]
#[ntest::timeout(30_000)]
fn the_debugger_drives_the_engine_clock_from_its_shortcuts() {
    use aurora::app::debug::InspectorMode;
    use aurora::app::debug::VisualDebugger;
    use aurora::app::debug_toggle::apply_debug_key;
    use aurora::app::extension::AppExtension;
    use winit::keyboard::{Key, NamedKey};

    let mut dbg = VisualDebugger::new();
    let mut h = Harness::page(Slide, 400, 200).with_delta(1.0 / 60.0);
    h.steps(2);

    // At real time the debugger has nothing to tell the engine, and must not ask for a frame
    // just to say so.
    assert!(
        !dbg.on_update(h.engine()),
        "an unchanged rate must not request a redraw"
    );
    assert!((h.engine().time_scale() - 1.0).abs() < 1e-6);

    // Press the freeze key the way the event loop would, then let the debugger push it.
    let mut mode = InspectorMode::Off;
    let mut hud = true;
    let mut scale = dbg.time_scale();
    let mut step = false;
    assert!(apply_debug_key(
        &Key::Named(NamedKey::F8),
        &mut mode,
        &mut hud,
        &mut scale,
        &mut step
    ));
    assert_eq!(scale, 0.0, "F8 must stop the clock");
    dbg.set_time_scale(scale);

    assert!(
        dbg.on_update(h.engine()),
        "changing the rate must request the frame that shows it"
    );
    assert!(
        h.engine().time_scale() <= 1e-6,
        "the engine must actually be stopped, it is at {}",
        h.engine().time_scale()
    );

    // And the engine is genuinely frozen: a transition triggered after the freeze never runs.
    h.click("card");
    h.steps(4);
    let held = h.transform("card").tx;
    for _ in 0..20 {
        h.step();
    }
    assert_eq!(
        h.transform("card").tx,
        held,
        "a stopped clock must not advance the animation"
    );

    // Restoring real time has to reach the engine as well, or the app stays stuck.
    dbg.set_time_scale(1.0);
    assert!(dbg.on_update(h.engine()));
    assert!((h.engine().time_scale() - 1.0).abs() < 1e-6);
    h.steps(40);
    assert!(
        h.transform("card").tx > held + 20.0,
        "the animation must resume once time is restored"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_frozen_transition_can_be_walked_one_frame_at_a_time() {
    // The reason to freeze at all. Without a step, a stopped clock shows one frame forever and
    // there is nothing to inspect.
    let mut h = Harness::page(Slide, 400, 200).with_delta(1.0 / 60.0);
    h.steps(2);
    h.click("card");
    h.steps(3);
    h.pause();

    let start = h.transform("card").tx;
    let mut positions = vec![start];
    for _ in 0..4 {
        h.step_once();
        positions.push(h.transform("card").tx);
    }

    for pair in positions.windows(2) {
        assert!(
            pair[1] > pair[0],
            "each step must advance the animation: {positions:?}"
        );
    }
    // One 120Hz quantum of a 400ms tween over 120px is 2.5px, so four steps move 10px.
    assert!(
        (positions[4] - positions[0] - 10.0).abs() <= 2.5,
        "four single-frame steps should move about 10px, they moved {:.2}: {positions:?}",
        positions[4] - positions[0]
    );

    // And stepping stops the clock again: the position must not drift afterwards.
    let held = positions[4];
    for _ in 0..10 {
        h.step();
    }
    assert!(
        (h.transform("card").tx - held).abs() < 1e-3,
        "an ordinary frame while frozen must still advance nothing"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_step_is_ignored_while_time_is_already_running() {
    // Otherwise pressing step mid-animation would jump a frame for no visible reason.
    let mut h = Harness::page(Slide, 400, 200).with_delta(1.0 / 60.0);
    h.steps(2);
    h.click("card");

    let before = h.engine().clock_debug_simulated();
    h.step_once();
    let after = h.engine().clock_debug_simulated();

    let per_frame = 1.0 / 60.0;
    assert!(
        (after - before - per_frame).abs() < 1.5 / 120.0,
        "a running clock should advance by its own delta, not by an extra quantum: {}",
        after - before
    );
}

#[test]
#[ntest::timeout(30_000)]
fn the_hud_names_the_rate_only_when_it_is_unusual() {
    use aurora::app::debug_hud::{time_scale_color, time_scale_label};

    assert!(
        time_scale_label(1.0).is_none(),
        "at real time the badge must be absent, so its appearance is the message"
    );
    assert_eq!(time_scale_label(0.0).as_deref(), Some("HOLD"));
    assert_eq!(time_scale_label(0.1).as_deref(), Some("0.10X"));
    assert_ne!(
        time_scale_color(0.0),
        time_scale_color(0.1),
        "stopped and slowed must not look alike"
    );
}

/// The debugger has to name its own frame-stepping shortcut.
///
/// A frozen clock reports "HOLD" and stops there, which tells you time stopped but not that the
/// stopped state is the one a transition can be walked frame by frame through -- the reason to
/// freeze in the first place. The hint line is the only place that is discoverable, so it has to
/// appear exactly when time is not running normally and name the step key.
#[test]
#[ntest::timeout(30_000)]
fn the_time_control_hint_names_the_step_key_while_frozen() {
    use aurora::app::debug_hud::time_control_hint;

    assert_eq!(time_control_hint(1.0), None, "no hint at normal speed");

    let frozen = time_control_hint(0.0).expect("a frozen clock must explain itself");
    assert!(
        frozen.contains('.') && frozen.contains("STEP"),
        "the frozen hint must name the frame-step key, got {frozen:?}"
    );

    // Slowed but running: the step key would do nothing there, so it must not be advertised.
    let slowed = time_control_hint(0.25).expect("a slowed clock must explain itself");
    assert!(
        !slowed.contains("STEP"),
        "stepping is meaningless while time runs, so it must not be offered: {slowed:?}"
    );
    assert!(
        slowed.contains("FREEZE"),
        "the slowed hint should still name the freeze key"
    );
}
