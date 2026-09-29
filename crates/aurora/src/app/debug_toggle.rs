// Single responsibility: Keyboard shortcut evaluation for visual debugger inspector modes.

use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{Key, NamedKey};

use crate::app::debug::InspectorMode;

/// Evaluates a window keyboard event against debugging shortcut bindings.
pub fn handle_debug_key(
    event: &WindowEvent,
    mode: &mut InspectorMode,
    show_hud: &mut bool,
) -> bool {
    if let WindowEvent::KeyboardInput {
        event: key_event, ..
    } = event
    {
        if key_event.state == ElementState::Pressed {
            return toggle_key(&key_event.logical_key, mode, show_hud);
        }
    }
    false
}

fn toggle_key(key: &Key, mode: &mut InspectorMode, show_hud: &mut bool) -> bool {
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
        _ => false,
    }
}
