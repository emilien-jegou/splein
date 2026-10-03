// Single responsibility: Keyboard shortcut evaluation for visual debugger inspector modes.

use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{Key, NamedKey};

use crate::app::debug::InspectorMode;

/// Rates the slow-motion control steps through, slowest first.
///
/// Discrete steps rather than a free-running multiplier: a debugger key that lands on a
/// memorable fraction is one you can reach without looking, and the ends are the two rates
/// anyone actually wants — a crawl to watch a spring's last few frames, and real time.
pub const TIME_SCALE_STEPS: [f32; 5] = [0.05, 0.1, 0.25, 0.5, 1.0];

/// Evaluates a window keyboard event against debugging shortcut bindings.
pub fn handle_debug_key(
    event: &WindowEvent,
    mode: &mut InspectorMode,
    show_hud: &mut bool,
    time_scale: &mut f32,
    step: &mut bool,
) -> bool {
    if let WindowEvent::KeyboardInput {
        event: key_event, ..
    } = event
    {
        if key_event.state == ElementState::Pressed {
            return apply_debug_key(&key_event.logical_key, mode, show_hud, time_scale, step);
        }
    }
    false
}

/// Applies a debug shortcut for a logical key.
///
/// Split from [`handle_debug_key`] because a `KeyEvent` cannot be constructed outside winit --
/// it carries a private extra payload -- so folding the window-event shape into this function
/// would make every binding here untestable.
pub fn apply_debug_key(
    key: &Key,
    mode: &mut InspectorMode,
    show_hud: &mut bool,
    time_scale: &mut f32,
    step: &mut bool,
) -> bool {
    toggle_key(key, mode, show_hud, time_scale, step)
}

/// The next rate slower than `scale`, or the slowest available.
fn slower(scale: f32) -> f32 {
    TIME_SCALE_STEPS
        .iter()
        .rev()
        .find(|s| **s < scale - 1e-4)
        .copied()
        .unwrap_or(TIME_SCALE_STEPS[0])
}

/// The next rate faster than `scale`, or real time.
fn faster(scale: f32) -> f32 {
    TIME_SCALE_STEPS
        .iter()
        .find(|s| **s > scale + 1e-4)
        .copied()
        .unwrap_or(1.0)
}

/// Whether a scale counts as frozen, for the HUD's benefit.
///
/// Compared with a tolerance so a value that is merely very small still reads as running; a
/// debugger that claims to be frozen while time is trickling is worse than one that says
/// 0.05x.
pub fn is_frozen(scale: f32) -> bool {
    scale <= 1e-4
}

fn toggle_key(
    key: &Key,
    mode: &mut InspectorMode,
    show_hud: &mut bool,
    time_scale: &mut f32,
    step: &mut bool,
) -> bool {
    match key {
        Key::Named(NamedKey::F12) => {
            *mode = if *mode == InspectorMode::DamageHeatmap {
                InspectorMode::Off
            } else {
                InspectorMode::DamageHeatmap
            };
            true
        }
        Key::Named(NamedKey::F11) => {
            *mode = if *mode == InspectorMode::LayoutAndBoundaries {
                InspectorMode::Off
            } else {
                InspectorMode::LayoutAndBoundaries
            };
            true
        }
        Key::Named(NamedKey::F9) => {
            *mode = if *mode == InspectorMode::StackingAndHierarchy {
                InspectorMode::Off
            } else {
                InspectorMode::StackingAndHierarchy
            };
            true
        }
        Key::Named(NamedKey::F10) => {
            *show_hud = !*show_hud;
            true
        }
        // Freeze on one key, because watching a transition frame by frame is the reason to
        // slow time down in the first place, and hunting for a bracket key while it is
        // running is exactly the distraction the debugger should not cause.
        Key::Named(NamedKey::F8) => {
            *time_scale = if is_frozen(*time_scale) { 1.0 } else { 0.0 };
            true
        }
        Key::Character(c) if c.as_str() == "[" => {
            *time_scale = if is_frozen(*time_scale) {
                TIME_SCALE_STEPS[1]
            } else {
                slower(*time_scale)
            };
            true
        }
        Key::Character(c) if c.as_str() == "]" => {
            *time_scale = faster(*time_scale);
            true
        }
        // One frame forward. Bound separately from the freeze key because it only means
        // anything while stopped, and putting it next to the speed controls would suggest it
        // works at any rate -- where it would silently do nothing.
        Key::Character(c) if c.as_str() == "." => {
            *step = true;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ntest::timeout(30_000)]
    fn stepping_down_walks_the_presets_and_stops_at_the_slowest() {
        let mut scale = 1.0f32;
        scale = slower(scale);
        assert!((scale - 0.5).abs() < 1e-6);
        scale = slower(scale);
        assert!((scale - 0.25).abs() < 1e-6);
        for _ in 0..8 {
            scale = slower(scale);
        }
        assert!(
            (scale - TIME_SCALE_STEPS[0]).abs() < 1e-6,
            "stepping down must stop at the slowest preset, not run past it into zero: {scale}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn stepping_up_walks_the_presets_and_stops_at_real_time() {
        let mut scale = TIME_SCALE_STEPS[0];
        scale = faster(scale);
        assert!((scale - 0.1).abs() < 1e-6);
        for _ in 0..8 {
            scale = faster(scale);
        }
        assert!(
            (scale - 1.0).abs() < 1e-6,
            "stepping up must stop at real time: {scale}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_frozen_scale_steps_back_into_the_presets_rather_than_staying_stuck() {
        let mut scale = 0.0f32;
        scale = faster(scale);
        assert!(
            (scale - TIME_SCALE_STEPS[0]).abs() < 1e-6,
            "stepping up from frozen must resume, got {scale}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn only_a_genuinely_stopped_clock_reads_as_frozen() {
        assert!(is_frozen(0.0));
        assert!(!is_frozen(0.05), "a crawl is still running and must say so");
        assert!(!is_frozen(1.0));
    }

    fn press(key: Key, scale: &mut f32) -> bool {
        press_with_step(key, scale, &mut false)
    }

    fn press_with_step(key: Key, scale: &mut f32, step: &mut bool) -> bool {
        let mut mode = InspectorMode::Off;
        let mut hud = true;
        apply_debug_key(&key, &mut mode, &mut hud, scale, step)
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn the_freeze_key_stops_and_resumes_the_clock() {
        let mut scale = 1.0f32;
        assert!(press(Key::Named(NamedKey::F8), &mut scale));
        assert!(is_frozen(scale), "F8 must stop the clock, it is at {scale}");

        assert!(press(Key::Named(NamedKey::F8), &mut scale));
        assert!(
            (scale - 1.0).abs() < 1e-6,
            "F8 again must restore real time, it is at {scale}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn the_bracket_keys_walk_the_clock_up_and_down() {
        let mut scale = 1.0f32;
        assert!(press(Key::Character("[".into()), &mut scale));
        assert!(
            (scale - 0.5).abs() < 1e-6,
            "[ must slow the clock, it is at {scale}"
        );
        assert!(press(Key::Character("]".into()), &mut scale));
        assert!(
            (scale - 1.0).abs() < 1e-6,
            "] must speed it back up, it is at {scale}"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_frozen_clock_can_be_resumed_by_stepping_up_rather_than_only_by_the_freeze_key() {
        // Otherwise a frozen engine is a dead end that needs the exact key that froze it.
        let mut scale = 0.0f32;
        assert!(press(Key::Character("]".into()), &mut scale));
        assert!(!is_frozen(scale), "stepping up must restart a frozen clock");
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn the_step_key_asks_for_exactly_one_frame() {
        let mut scale = 0.0f32;
        let mut step = false;
        assert!(press_with_step(
            Key::Character(".".into()),
            &mut scale,
            &mut step
        ));
        assert!(step, "'.' must request a frame");
        assert!(
            is_frozen(scale),
            "stepping must not resume the clock on its own"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn an_unrelated_key_is_left_to_the_application() {
        let mut scale = 1.0f32;
        assert!(!press(Key::Character("q".into()), &mut scale));
        assert!((scale - 1.0).abs() < 1e-6);
    }
}
