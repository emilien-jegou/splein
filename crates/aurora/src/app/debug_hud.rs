// Single responsibility: Composes telemetry diagnostics HUD panels onto pixel buffers.

use crate::app::debug::InspectorMode;
use crate::app::debug_draw::{draw_tint, draw_wireframe};
use crate::app::debug_font::draw_text_scaled;
use crate::foundation::ResolvedRect;
use crate::runtime::FrameDiagnostics;

/// Renders HUD diagnostic telemetry panel with active backend name and metrics breakdown.
pub fn draw_hud_panel(
    buffer: &mut [u32],
    is_idle: bool,
    display_fps: u32,
    backend_tag: &str,
    diag: &FrameDiagnostics,
    mode: InspectorMode,
    stride: usize,
    height: usize,
) {
    let hud_rect = ResolvedRect::new(12.0, 12.0, 310.0, 48.0);
    draw_tint(buffer, &hud_rect, 0x000F172A, 240, stride, height);
    draw_wireframe(buffer, &hud_rect, 0x00334155, 1, stride, height);

    let (mode_color, mode_name) = match mode {
        InspectorMode::DamageHeatmap => (0x00F97316, "DMG"),
        InspectorMode::LayoutAndBoundaries => (0x0010B981, "LAY"),
        InspectorMode::StackingAndHierarchy => (0x00A855F7, "STK"),
        InspectorMode::Off => (0x00475569, "OFF"),
    };
    draw_tint(buffer, &ResolvedRect::new(20.0, 20.0, 12.0, 12.0), mode_color, 255, stride, height);
    draw_wireframe(buffer, &ResolvedRect::new(20.0, 20.0, 12.0, 12.0), 0x00FFFFFF, 1, stride, height);

    let total_us = diag.timings.total.as_micros();
    let ms_val = total_us as f32 / 1000.0;
    let color = if is_idle { 0x0094A3B8 } else if ms_val < 8.33 { 0x0022C55E } else if ms_val < 16.66 { 0x00EAB308 } else { 0x00EF4444 };

    let (fps_str, ms_str) = if is_idle {
        ("IDLE".into(), "0.0MS".into())
    } else {
        (format!("{}FPS", display_fps.min(9999)), format!("{:.1}MS", ms_val))
    };

    // Row 1: Backend Badge (GPU / CPU) + Mode + FPS + Compute Time
    let is_gpu = backend_tag.contains("GPU");
    let badge_color = if is_gpu { 0x0010B981 } else { 0x0038BDF8 };
    draw_text_scaled(buffer, backend_tag, 38, 20, 2, badge_color, stride, height);
    draw_text_scaled(buffer, "|", 70, 20, 2, 0x00475569, stride, height);
    draw_text_scaled(buffer, mode_name, 82, 20, 2, 0x0094A3B8, stride, height);
    draw_text_scaled(buffer, &fps_str, 126, 20, 2, 0x0022C55E, stride, height);
    draw_text_scaled(buffer, &ms_str, 220, 20, 2, color, stride, height);

    // Row 2: Frame budget headroom line
    let bar_c = if total_us < 8_333 { 0x0022C55E } else if total_us < 16_666 { 0x00EAB308 } else { 0x00EF4444 };
    let bar_w = ((total_us as f32 / 16_666.0).min(1.0) * 286.0).max(6.0);
    draw_tint(buffer, &ResolvedRect::new(20.0, 40.0, bar_w, 3.0), bar_c, 255, stride, height);
}
