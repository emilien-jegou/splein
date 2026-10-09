// Single responsibility: Composes telemetry diagnostics HUD panels onto pixel buffers.

use crate::app::debug::InspectorMode;
use crate::app::debug_draw::{draw_tint, draw_wireframe};
use crate::app::debug_font::draw_text_scaled;
use crate::foundation::ResolvedRect;
use crate::runtime::FrameDiagnostics;

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

/// Always-on keybind strip for the demo runner (switch/combine + time controls).
pub fn keybind_hint() -> &'static str {
    "1-4 ENGINE  TAB CYCLE  C COMBINE  H HUD  F8 FREEZE  . STEP  [ ] SPEED  F9/F11/F12 VIEW  S CAP"
}

/// Renders HUD diagnostic telemetry panel with active backend name and metrics breakdown.
#[allow(clippy::too_many_arguments)]
pub fn draw_hud_panel(
    buffer: &mut [u32],
    is_idle: bool,
    display_fps: u32,
    avg_fps_5s: f32,
    backend_tag: &str,
    diag: &FrameDiagnostics,
    mode: InspectorMode,
    time_scale: f32,
    stride: usize,
    height: usize,
) {
    let hud_rect = ResolvedRect::new(12.0, 12.0, 560.0, 64.0);
    draw_tint(buffer, &hud_rect, 0x000F172A, 240, stride, height);
    draw_wireframe(buffer, &hud_rect, 0x00334155, 1, stride, height);

    let (mode_color, mode_name) = match mode {
        InspectorMode::DamageHeatmap => (0x00F97316, "DMG"),
        InspectorMode::LayoutAndBoundaries => (0x0010B981, "LAY"),
        InspectorMode::StackingAndHierarchy => (0x00A855F7, "STK"),
        InspectorMode::Off => (0x00475569, "OFF"),
    };
    draw_tint(
        buffer,
        &ResolvedRect::new(20.0, 20.0, 12.0, 12.0),
        mode_color,
        255,
        stride,
        height,
    );
    draw_wireframe(
        buffer,
        &ResolvedRect::new(20.0, 20.0, 12.0, 12.0),
        0x00FFFFFF,
        1,
        stride,
        height,
    );

    let total_us = diag.timings.total.as_micros();
    let ms_val = total_us as f32 / 1000.0;
    let color = if is_idle {
        0x0094A3B8
    } else if ms_val < 8.33 {
        0x0022C55E
    } else if ms_val < 16.66 {
        0x00EAB308
    } else {
        0x00EF4444
    };

    let (fps_str, ms_str) = if is_idle {
        ("IDLE".into(), "0.0MS".into())
    } else {
        (
            format!("{}FPS AVG5:{:.0}", display_fps.min(9999), avg_fps_5s),
            format!("{:.1}MS", ms_val),
        )
    };

    // Row 1: Backend Badge + Mode + FPS (+5s avg) + Compute Time
    let badge_color = match backend_tag {
        "vello-gpu" | "wgpu" => 0x0010B981,
        "vello-cpu" => 0x00F59E0B,
        _ => 0x0038BDF8,
    };
    draw_text_scaled(buffer, backend_tag, 38, 20, 2, badge_color, stride, height);
    draw_text_scaled(buffer, "|", 150, 20, 2, 0x00475569, stride, height);
    draw_text_scaled(buffer, mode_name, 162, 20, 2, 0x0094A3B8, stride, height);
    draw_text_scaled(buffer, &fps_str, 206, 20, 2, 0x0022C55E, stride, height);
    draw_text_scaled(buffer, &ms_str, 430, 20, 2, color, stride, height);

    if let Some(label) = time_scale_label(time_scale) {
        draw_text_scaled(
            buffer,
            &label,
            268,
            20,
            2,
            time_scale_color(time_scale),
            stride,
            height,
        );
    }

    // Row 2: Frame budget headroom line
    let bar_c = if total_us < 8_333 {
        0x0022C55E
    } else if total_us < 16_666 {
        0x00EAB308
    } else {
        0x00EF4444
    };
    let bar_w = ((total_us as f32 / 16_666.0).min(1.0) * 536.0).max(6.0);
    draw_tint(
        buffer,
        &ResolvedRect::new(20.0, 40.0, bar_w, 3.0),
        bar_c,
        255,
        stride,
        height,
    );

    // The hint sits below the budget bar, at half the badge's scale so the whole line fits
    // without pushing the HUD wider than the bar already is.
    if let Some(hint) = time_control_hint(time_scale) {
        draw_text_scaled(buffer, hint, 20, 48, 1, 0x0094A3B8, stride, height);
    }
    // Row 3: always-on keybind strip for engine switch / combine.
    draw_text_scaled(buffer, keybind_hint(), 20, 58, 1, 0x0064748B, stride, height);
}
