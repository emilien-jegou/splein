// Single responsibility: Emits display list overlay chunks for GPU compute rasterizers.

use crate::app::debug::InspectorMode;
use crate::foundation::{Appearance, Color, Fill, Point, ResolvedRect, Stroke};
use crate::runtime::FrameDiagnostics;
use crate::scene::{SceneChunk, SceneCommand};
use crate::text::TextContext;

/// Emits inspector mode visual cues directly into an overlay scene chunk for GPU backends.
pub fn emit_inspector_overlay(
    chunk: &mut SceneChunk,
    mode: InspectorMode,
    diag: &FrameDiagnostics,
) {
    match mode {
        InspectorMode::DamageHeatmap => {
            if let Some(preserved) = &diag.spatial.preserved_canvas_rect {
                chunk.push(SceneCommand::DrawRect {
                    rect: *preserved,
                    appearance: Appearance::EMPTY
                        .with_stroke(Stroke::inside(1.0, Color::hex(0x22C55E))),
                });
            }
            for removed in &diag.spatial.removed_rects {
                chunk.push(SceneCommand::DrawRect {
                    rect: *removed,
                    appearance: Appearance::EMPTY
                        .with_fill(Fill::solid(Color::hex_alpha(0xDC2626, 0.25)))
                        .with_stroke(Stroke::inside(1.0, Color::hex(0xEF4444))),
                });
            }
            for rect in &diag.spatial.damaged_rects {
                chunk.push(SceneCommand::DrawRect {
                    rect: *rect,
                    appearance: Appearance::EMPTY
                        .with_fill(Fill::solid(Color::hex_alpha(0xF97316, 0.25)))
                        .with_stroke(Stroke::inside(2.0, Color::hex(0xFF4500))),
                });
            }
        }
        InspectorMode::LayoutAndBoundaries => {
            for rect in &diag.layout.cached_nodes {
                chunk.push(SceneCommand::DrawRect {
                    rect: *rect,
                    appearance: Appearance::EMPTY
                        .with_stroke(Stroke::inside(1.0, Color::hex(0x06B6D4))),
                });
            }
            for rect in &diag.layout.recomputed_nodes {
                chunk.push(SceneCommand::DrawRect {
                    rect: *rect,
                    appearance: Appearance::EMPTY
                        .with_fill(Fill::solid(Color::hex_alpha(0xEAB308, 0.20)))
                        .with_stroke(Stroke::inside(2.0, Color::hex(0xFACC15))),
                });
            }
            for boundary in &diag.layout.boundary_nodes {
                chunk.push(SceneCommand::DrawRect {
                    rect: *boundary,
                    appearance: Appearance::EMPTY
                        .with_stroke(Stroke::inside(2.0, Color::hex(0x10B981))),
                });
            }
        }
        InspectorMode::StackingAndHierarchy => {
            for rect in &diag.compile.context_bounds {
                chunk.push(SceneCommand::DrawRect {
                    rect: *rect,
                    appearance: Appearance::EMPTY
                        .with_fill(Fill::solid(Color::hex_alpha(0xA855F7, 0.20)))
                        .with_stroke(Stroke::inside(2.0, Color::hex(0xC084FC))),
                });
            }
        }
        InspectorMode::Off => {}
    }
}

/// Emits vector HUD panel telemetry card directly into a scene chunk for GPU rasterizers.
pub fn emit_hud_overlay(
    chunk: &mut SceneChunk,
    is_idle: bool,
    fps: u32,
    backend_name: &str,
    diag: &FrameDiagnostics,
    mode: InspectorMode,
    text_ctx: &TextContext,
) {
    // 1. Panel Container
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(12.0, 12.0, 310.0, 48.0),
        appearance: Appearance::EMPTY
            .with_fill(Fill::solid(Color::hex_alpha(0x0F172A, 0.94)))
            .with_stroke(Stroke::inside(1.0, Color::hex(0x334155))),
    });

    // 2. Mode Indicator Square
    let (mode_color, mode_name) = match mode {
        InspectorMode::DamageHeatmap => (Color::hex(0xF97316), "DMG"),
        InspectorMode::LayoutAndBoundaries => (Color::hex(0x10B981), "LAY"),
        InspectorMode::StackingAndHierarchy => (Color::hex(0xA855F7), "STK"),
        InspectorMode::Off => (Color::hex(0x475569), "OFF"),
    };
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(20.0, 20.0, 12.0, 12.0),
        appearance: Appearance::EMPTY
            .with_fill(Fill::solid(mode_color))
            .with_stroke(Stroke::inside(1.0, Color::WHITE)),
    });

    // 3. Segmented Metrics Row
    let total_us = diag.timings.total.as_micros();
    let ms_val = total_us as f32 / 1000.0;
    let ms_color = if is_idle {
        Color::hex(0x94A3B8)
    } else if ms_val < 8.33 {
        Color::hex(0x22C55E)
    } else if ms_val < 16.66 {
        Color::hex(0xEAB308)
    } else {
        Color::hex(0xEF4444)
    };
    let (fps_str, ms_str) = if is_idle {
        ("IDLE".into(), "0.0MS".into())
    } else {
        (format!("{}FPS", fps.min(9999)), format!("{:.1}MS", ms_val))
    };

    emit_label(
        chunk,
        backend_name,
        Point::new(38.0, 20.0),
        Color::hex(0x10B981),
        text_ctx,
    );
    emit_label(
        chunk,
        "|",
        Point::new(70.0, 20.0),
        Color::hex(0x475569),
        text_ctx,
    );
    emit_label(
        chunk,
        mode_name,
        Point::new(82.0, 20.0),
        Color::hex(0x94A3B8),
        text_ctx,
    );
    emit_label(
        chunk,
        &fps_str,
        Point::new(126.0, 20.0),
        Color::hex(0x22C55E),
        text_ctx,
    );
    emit_label(chunk, &ms_str, Point::new(220.0, 20.0), ms_color, text_ctx);

    // 4. Frame Budget Headroom Line
    let bar_w = ((total_us as f32 / 16_666.0).min(1.0) * 286.0).max(6.0);
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(20.0, 40.0, bar_w, 3.0),
        appearance: Appearance::EMPTY.with_fill(Fill::solid(ms_color)),
    });
}

fn emit_label(
    chunk: &mut SceneChunk,
    content: &str,
    origin: Point,
    color: Color,
    text_ctx: &TextContext,
) {
    let cfg = crate::text::TextConfig {
        content: content.into(),
        font_id: None,
        size: 11.0,
        weight: 700,
        line_height: 14.0,
        letter_spacing: 0.0,
        color,
    };
    let layout = text_ctx.shape_config(&cfg, crate::foundation::Constraints::loose(80.0, 16.0));
    chunk.push(SceneCommand::DrawText {
        origin,
        layout,
        color,
    });
}
