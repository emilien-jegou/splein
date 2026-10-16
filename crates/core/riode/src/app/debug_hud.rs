// Single responsibility: HUD label helpers shared by the scene-chunk overlay.

/// Short badge naming the animation clock's rate, or `None` at real time.
///
/// Absent rather than reading `1.0x` when nothing is unusual, so the badge appearing is itself
/// the signal that time is no longer running normally.
pub fn time_scale_label(scale: f32) -> Option<String> {
    if (scale - 1.0).abs() < 1e-4 {
        return None;
    }
    if scale <= 1e-4 {
        return Some("HOLD".into());
    }
    Some(format!("{scale:.2}X"))
}

/// The debugger's own shortcuts, shown whenever time is not running normally.
///
/// A debugger that makes you remember a key is a debugger you stop reaching for. These three are
/// the ones that cannot be guessed from what is on screen: nothing about a frozen clock says the
/// transition can be walked a frame at a time, and that is the reason to freeze it. `None` at
/// normal speed, so the hint appears exactly when it is relevant and stays out of the way.
pub fn time_control_hint(scale: f32) -> Option<&'static str> {
    if (scale - 1.0).abs() < 1e-4 {
        return None;
    }
    if scale <= 1e-4 {
        Some("F8 RUN  . STEP  [ ] SPEED")
    } else {
        Some("F8 FREEZE  [ ] SPEED")
    }
}

/// Colour the time-scale badge is drawn in: amber when slowed, red when stopped.
pub fn time_scale_color(scale: f32) -> u32 {
    if scale <= 1e-4 {
        0x00EF4444
    } else {
        0x00F59E0B
    }
}

/// Always-on keybind strip for the demo runner (switch + time controls).
pub fn keybind_hint() -> &'static str {
    "F5 DEMOS  F7 ENGINE  F8 FREEZE  . STEP  [ ] SPEED  B SNAP-Y  F9/F11/F12 VIEW  S CAP  T TRACY  ESC EXIT"
}
