// Isolates gesture dragging, shape previews, and freehand segment collection.

use super::selection::resize_rect;
use super::tools::build_shape_preview;
use crate::domain::canvas::{Canvas, Rgba};
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::{Aabb, CubicBezierSegment, Handle, InputSample, Vec2};

pub enum DragState {
    None,
    Move { idxs: Vec<usize>, start: Vec2, current: Vec2 },
    Resize { idxs: Vec<usize>, base: Aabb, handle: Handle, current: Vec2 },
    Band { start: Vec2, current: Vec2 },
}

pub struct GestureEngine {
    pub drag: DragState,
    pub shape_start: Option<Vec2>,
    pub unbaked_segments: Vec<(CubicBezierSegment, Rgba, f32)>,
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
            ActiveTool::Pointer => match &mut self.drag {
                DragState::Move { current, .. } => *current = pos,
                DragState::Resize { current, .. } => *current = pos,
                DragState::Band { current, .. } => *current = pos,
                DragState::None => {}
            },
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

    /// Completes the active drag, returning elements captured by a band drag.
    pub fn on_up(&mut self, canvas: &mut Canvas) -> Option<Vec<usize>> {
        self.shape_start = None;
        let captured = match std::mem::replace(&mut self.drag, DragState::None) {
            DragState::Move { idxs, start, current } => {
                let delta = Vec2::new(current.x - start.x, current.y - start.y);
                if delta.distance_squared(Vec2::ZERO) > 1.0 {
                    for &idx in &idxs {
                        canvas.translate_element(idx, delta);
                    }
                }
                None
            }
            DragState::Resize { idxs, base, handle, current } => {
                let target = resize_rect(base, handle, current);
                for &idx in &idxs {
                    canvas.map_element(idx, base, target);
                }
                None
            }
            DragState::Band { start, current } => {
                let region = Aabb::span(start, current);
                let dragged = region.max.x - region.min.x > 3.0 || region.max.y - region.min.y > 3.0;
                Some(if dragged { canvas.elements_in(region) } else { Vec::new() })
            }
            DragState::None => None,
        };
        self.unbaked_segments.extend(canvas.commit_active());
        captured
    }

    /// Starts a move drag over the given canvas elements.
    pub fn begin_move(&mut self, idxs: Vec<usize>, pos: Vec2) {
        self.drag = DragState::Move { idxs, start: pos, current: pos };
    }

    /// Starts an interactive resize of the given canvas elements.
    pub fn begin_resize(&mut self, idxs: Vec<usize>, base: Aabb, handle: Handle, pos: Vec2) {
        self.drag = DragState::Resize { idxs, base, handle, current: pos };
    }

    /// Starts a rubber-band selection region at `pos`.
    pub fn begin_band(&mut self, pos: Vec2) {
        self.drag = DragState::Band { start: pos, current: pos };
    }

    /// Returns true while the pointer tool is moving, resizing, or banding.
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag, DragState::None)
    }

    /// Returns the moved element indices and delta while moving.
    pub fn move_preview(&self) -> Option<(&[usize], Vec2)> {
        match &self.drag {
            DragState::Move { idxs, start, current } => {
                Some((idxs.as_slice(), Vec2::new(current.x - start.x, current.y - start.y)))
            }
            _ => None,
        }
    }

    /// Returns the element indices and base/target rects while resizing.
    pub fn resize_preview(&self) -> Option<(&[usize], Aabb, Aabb)> {
        match &self.drag {
            DragState::Resize { idxs, base, handle, current } => {
                Some((idxs.as_slice(), *base, resize_rect(*base, *handle, *current)))
            }
            _ => None,
        }
    }

    /// Returns the region of an active band drag.
    pub fn band_rect(&self) -> Option<Aabb> {
        match &self.drag {
            DragState::Band { start, current } => Some(Aabb::span(*start, *current)),
            _ => None,
        }
    }
}
