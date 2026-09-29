// Isolates gesture dragging, shape previews, and freehand segment collection.

use super::tools::build_shape_preview;
use crate::domain::canvas::{Canvas, Rgba};
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::{CubicBezierSegment, InputSample, Vec2};

pub enum DragState {
    None,
    Element { idx: usize, start: Vec2, current: Vec2 },
}

pub struct GestureEngine {
    pub drag: DragState,
    pub shape_start: Option<Vec2>,
    pub unbaked_segments: Vec<(CubicBezierSegment, Rgba, f32, bool)>,
}

impl Default for GestureEngine {
    fn default() -> Self {
        Self {
            drag: DragState::None,
            shape_start: None,
            unbaked_segments: Vec::with_capacity(8),
        }
    }
}

impl GestureEngine {
    pub fn on_down(&mut self, pos: Vec2, pressure: f32, tool: ActiveTool, color: Rgba, canvas: &mut Canvas) {
        self.shape_start = Some(pos);
        let sample = InputSample::new(pos.x, pos.y, pressure);

        match tool {
            ActiveTool::Pointer => {
                if let Some(idx) = canvas.hit_test(pos, 14.0) {
                    self.drag = DragState::Element { idx, start: pos, current: pos };
                }
            }
            ActiveTool::Pen => canvas.begin_freehand(color, 3.5, false, sample),
            ActiveTool::Highlighter => canvas.begin_freehand(color.with_alpha(0.38), 20.0, true, sample),
            ActiveTool::Eraser => { canvas.erase_at(pos, 18.0); }
            _ => {
                if let Some(shape) = build_shape_preview(tool, pos, pos, color) {
                    canvas.update_shape_preview(shape);
                }
            }
        }
    }

    pub fn on_move(&mut self, pos: Vec2, pressure: f32, tool: ActiveTool, color: Rgba, canvas: &mut Canvas) {
        let sample = InputSample::new(pos.x, pos.y, pressure);

        match tool {
            ActiveTool::Pointer => {
                if let DragState::Element { idx, start, .. } = self.drag {
                    self.drag = DragState::Element { idx, start, current: pos };
                }
            }
            ActiveTool::Pen | ActiveTool::Highlighter => {
                canvas.extend_freehand(sample);
                if let Some(chunk) = canvas.take_pending_segment() {
                    self.unbaked_segments.push(chunk);
                }
            }
            ActiveTool::Eraser => { canvas.erase_at(pos, 18.0); }
            _ => {
                if let Some(start) = self.shape_start {
                    if let Some(shape) = build_shape_preview(tool, start, pos, color) {
                        canvas.update_shape_preview(shape);
                    }
                }
            }
        }
    }

    pub fn on_up(&mut self, canvas: &mut Canvas) {
        self.shape_start = None;
        if let DragState::Element { idx, start, current } = std::mem::replace(&mut self.drag, DragState::None) {
            let delta = Vec2::new(current.x - start.x, current.y - start.y);
            if delta.distance_squared(Vec2::ZERO) > 1.0 {
                canvas.translate_element(idx, delta);
            }
        }
        self.unbaked_segments.extend(canvas.commit_active());
    }

    pub fn live_drag_delta(&self) -> Option<(usize, Vec2)> {
        match self.drag {
            DragState::Element { idx, start, current } => Some((idx, Vec2::new(current.x - start.x, current.y - start.y))),
            DragState::None => None,
        }
    }
}
