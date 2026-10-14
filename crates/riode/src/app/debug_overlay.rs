// Single responsibility: Zero-allocation direct presentation scanline overlay blitter for visual cues.

use crate::app::debug::InspectorMode;
use crate::app::debug_draw::{draw_double_border, draw_tint, draw_wireframe};
use crate::runtime::FrameDiagnostics;

/// Renders visual inspection wireframes directly onto the output presentation buffer.
pub fn draw_inspector_overlay(
    buf: &mut [u32],
    mode: InspectorMode,
    diag: &FrameDiagnostics,
    stride: usize,
    height: usize,
) {
    match mode {
        InspectorMode::DamageHeatmap => {
            if let Some(preserved) = &diag.spatial.preserved_canvas_rect {
                draw_wireframe(buf, preserved, 0x0022C55E, 1, stride, height);
            }
            for removed in &diag.spatial.removed_rects {
                draw_tint(buf, removed, 0x00DC2626, 64, stride, height);
                draw_wireframe(buf, removed, 0x00EF4444, 1, stride, height);
            }
            for rect in &diag.spatial.damaged_rects {
                draw_tint(buf, rect, 0x00F97316, 64, stride, height);
                draw_wireframe(buf, rect, 0x00FF4500, 2, stride, height);
            }
        }
        InspectorMode::LayoutAndBoundaries => {
            for rect in &diag.layout.cached_nodes {
                draw_wireframe(buf, rect, 0x0006B6D4, 1, stride, height);
            }
            for rect in &diag.layout.recomputed_nodes {
                draw_tint(buf, rect, 0x00EAB308, 50, stride, height);
                draw_wireframe(buf, rect, 0x00FACC15, 2, stride, height);
            }
            for boundary in &diag.layout.boundary_nodes {
                draw_double_border(buf, boundary, 0x0010B981, stride, height);
            }
        }
        InspectorMode::StackingAndHierarchy => {
            for rect in &diag.compile.context_bounds {
                draw_tint(buf, rect, 0x00A855F7, 50, stride, height);
                draw_wireframe(buf, rect, 0x00C084FC, 2, stride, height);
            }
        }
        InspectorMode::Off => {}
    }
}
