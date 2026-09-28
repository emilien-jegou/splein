// crates/splein/src/app/session.rs

use crate::domain::canvas::{Canvas, DrawingElement};
use crate::domain::dock::{ActiveTool, Dock, DockInteraction};
use crate::domain::geometry::{Aabb, CubicBezierSegment, InputSample, Vec2};
use crate::render::{RenderEngine, Scene};

pub enum SessionAction {
    Redraw,
    ChangeInputPassthrough(bool),
    ToolChanged(ActiveTool),
}

pub struct OverlaySession<R: RenderEngine> {
    canvas: Canvas,
    dock: Dock,
    renderer: R,
    is_active: bool,
    width: u32,
    height: u32,
    shape_start: Option<Vec2>,
    dragged_element_idx: Option<usize>,
    drag_start_pos: Option<Vec2>,
    current_drag_pos: Option<Vec2>,
    hovered_element_idx: Option<usize>,
    hovered_dock_tool: Option<ActiveTool>,
    pre_erase_tool: Option<ActiveTool>,
    is_right_clicking: bool,
    accumulated_unbaked_segments: Vec<(CubicBezierSegment, crate::domain::canvas::Rgba, f32, bool)>,
}

impl<R: RenderEngine> OverlaySession<R> {
    pub fn new(renderer: R) -> Self {
        Self {
            canvas: Canvas::default(),
            dock: Dock::default(),
            renderer,
            is_active: false,
            width: 1920,
            height: 1080,
            shape_start: None,
            dragged_element_idx: None,
            drag_start_pos: None,
            current_drag_pos: None,
            hovered_element_idx: None,
            hovered_dock_tool: None,
            pre_erase_tool: None,
            is_right_clicking: false,
            accumulated_unbaked_segments: Vec::with_capacity(8),
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn set_dimensions(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.dock.position.x = ((width as f32) - self.dock.width).max(0.0) / 2.0;
        self.canvas.needs_full_rebuild = true;
    }

    pub fn toggle_active(&mut self) -> SessionAction {
        self.is_active = !self.is_active;
        if self.is_active {
            self.canvas.needs_full_rebuild = true;
        }
        SessionAction::ChangeInputPassthrough(!self.is_active)
    }

    pub fn clear(&mut self) -> SessionAction {
        self.canvas.clear();
        self.hovered_element_idx = None;
        self.dragged_element_idx = None;
        SessionAction::Redraw
    }

    pub fn undo(&mut self) -> SessionAction {
        self.canvas.undo();
        self.hovered_element_idx = None;
        SessionAction::Redraw
    }

    pub fn current_tool(&self) -> ActiveTool {
        self.dock.current_tool
    }

    pub fn select_tool(&mut self, tool: ActiveTool) -> SessionAction {
        self.dock.set_tool(tool);
        SessionAction::ToolChanged(tool)
    }

    pub fn select_color(&mut self, idx: usize) -> SessionAction {
        self.dock.set_color_idx(idx);
        SessionAction::Redraw
    }

    pub fn begin_temporary_erase(&mut self, pos: Vec2) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }
        self.is_right_clicking = true;
        if self.pre_erase_tool.is_none() && self.dock.current_tool != ActiveTool::Eraser {
            self.pre_erase_tool = Some(self.dock.current_tool);
        }
        self.dock.set_tool(ActiveTool::Eraser);
        self.canvas.erase_at(pos, 18.0);
        self.hovered_element_idx = None;
        SessionAction::ToolChanged(ActiveTool::Eraser)
    }

    pub fn end_temporary_erase(&mut self) -> SessionAction {
        self.is_right_clicking = false;
        let restored_tool = self.pre_erase_tool.take().unwrap_or(ActiveTool::Pen);
        self.dock.set_tool(restored_tool);
        SessionAction::ToolChanged(restored_tool)
    }

    pub fn is_point_over_dock(&self, p: Vec2) -> bool {
        self.dock.contains(p)
    }

    pub fn handle_pointer_down(&mut self, pos: Vec2, pressure: f32) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }

        if let Some(interaction) = self.dock.handle_pointer_down(pos) {
            return match interaction {
                DockInteraction::ClearRequested => self.clear(),
                DockInteraction::ToolSelected(tool) => SessionAction::ToolChanged(tool),
                _ => SessionAction::Redraw,
            };
        }

        self.shape_start = Some(pos);
        self.drag_start_pos = Some(pos);
        self.current_drag_pos = Some(pos);
        let sample = InputSample::new(pos.x, pos.y, pressure);
        let color = self.dock.active_color();

        match self.dock.current_tool {
            ActiveTool::Pointer => {
                self.dragged_element_idx = self.canvas.hit_test(pos, 14.0);
            }
            ActiveTool::Pen => {
                self.canvas.begin_freehand(color, 3.5, false, sample);
            }
            ActiveTool::Highlighter => {
                let high_color = color.with_alpha(0.38);
                self.canvas.begin_freehand(high_color, 20.0, true, sample);
            }
            ActiveTool::Line => {
                self.canvas.update_shape_preview(DrawingElement::Line {
                    start: pos,
                    end: pos,
                    color,
                    width: 3.5,
                    aabb: Aabb::from_point(pos),
                });
            }
            ActiveTool::Rect => {
                self.canvas.update_shape_preview(DrawingElement::Rect {
                    min: pos,
                    max: pos,
                    color,
                    width: 3.0,
                    aabb: Aabb::from_point(pos),
                });
            }
            ActiveTool::Ellipse => {
                self.canvas.update_shape_preview(DrawingElement::Ellipse {
                    center: pos,
                    rx: 0.0,
                    ry: 0.0,
                    color,
                    width: 3.0,
                    aabb: Aabb::from_point(pos),
                });
            }
            ActiveTool::Eraser => {
                self.canvas.erase_at(pos, 18.0);
                self.hovered_element_idx = None;
            }
            ActiveTool::Text | ActiveTool::SelectRegion => {}
        }
        SessionAction::Redraw
    }

    pub fn handle_pointer_move(&mut self, pos: Vec2, pressure: f32, is_down: bool) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }

        if self.is_right_clicking {
            self.canvas.erase_at(pos, 18.0);
            return SessionAction::Redraw;
        }

        if self.dock.is_dragging {
            self.dock.handle_pointer_move(pos);
            return SessionAction::Redraw;
        }

        if !is_down {
            let dock_hover = self.dock.get_hovered_tool(pos).map(|h| h.tool);
            let mut need_redraw = false;

            if self.hovered_dock_tool != dock_hover {
                self.hovered_dock_tool = dock_hover;
                need_redraw = true;
            }

            let padding = match self.dock.current_tool {
                ActiveTool::Pointer => 14.0,
                ActiveTool::Eraser => 16.0,
                _ => 0.0,
            };

            let new_canvas_hover = if padding > 0.0 && !self.is_point_over_dock(pos) {
                self.canvas.hit_test(pos, padding)
            } else {
                None
            };

            if self.hovered_element_idx != new_canvas_hover {
                self.hovered_element_idx = new_canvas_hover;
                need_redraw = true;
            }

            if need_redraw {
                return SessionAction::Redraw;
            }
            return SessionAction::Redraw;
        }

        let color = self.dock.active_color();
        let sample = InputSample::new(pos.x, pos.y, pressure);

        match self.dock.current_tool {
            ActiveTool::Pointer => {
                if self.dragged_element_idx.is_some() {
                    self.current_drag_pos = Some(pos);
                }
            }
            ActiveTool::Pen | ActiveTool::Highlighter => {
                self.canvas.extend_freehand(sample);
                if let Some(chunk) = self.canvas.take_pending_segment() {
                    self.accumulated_unbaked_segments.push(chunk);
                }
            }
            ActiveTool::Line => {
                if let Some(start) = self.shape_start {
                    let mut aabb = Aabb::from_point(start);
                    aabb.expand_with_point(pos);
                    self.canvas.update_shape_preview(DrawingElement::Line {
                        start,
                        end: pos,
                        color,
                        width: 3.5,
                        aabb,
                    });
                }
            }
            ActiveTool::Rect => {
                if let Some(start) = self.shape_start {
                    let min = Vec2::new(start.x.min(pos.x), start.y.min(pos.y));
                    let max = Vec2::new(start.x.max(pos.x), start.y.max(pos.y));
                    let aabb = Aabb::new(min, max);
                    self.canvas.update_shape_preview(DrawingElement::Rect {
                        min,
                        max,
                        color,
                        width: 3.0,
                        aabb,
                    });
                }
            }
            ActiveTool::Ellipse => {
                if let Some(start) = self.shape_start {
                    let center = Vec2::new((start.x + pos.x) / 2.0, (start.y + pos.y) / 2.0);
                    let rx = (pos.x - start.x).abs() / 2.0;
                    let ry = (pos.y - start.y).abs() / 2.0;
                    let aabb = Aabb::new(
                        Vec2::new(center.x - rx, center.y - ry),
                        Vec2::new(center.x + rx, center.y + ry),
                    );
                    self.canvas.update_shape_preview(DrawingElement::Ellipse {
                        center,
                        rx,
                        ry,
                        color,
                        width: 3.0,
                        aabb,
                    });
                }
            }
            ActiveTool::Eraser => {
                self.canvas.erase_at(pos, 18.0);
            }
            ActiveTool::Text | ActiveTool::SelectRegion => {}
        }
        SessionAction::Redraw
    }

    pub fn handle_pointer_up(&mut self) -> SessionAction {
        self.dock.handle_pointer_up();
        self.shape_start = None;

        if let (Some(idx), Some(start), Some(curr)) = (self.dragged_element_idx, self.drag_start_pos, self.current_drag_pos) {
            let delta = Vec2::new(curr.x - start.x, curr.y - start.y);
            if delta.distance_squared(Vec2::ZERO) > 1.0 {
                self.canvas.translate_element(idx, delta);
            }
        }

        self.dragged_element_idx = None;
        self.drag_start_pos = None;
        self.current_drag_pos = None;

        let final_segments = self.canvas.commit_active();
        self.accumulated_unbaked_segments.extend(final_segments);

        SessionAction::Redraw
    }

    pub fn render_to_buffer(&mut self, buffer: &mut [u8]) {
        let is_eraser = self.dock.current_tool == ActiveTool::Eraser;
        let live_drag_delta = if let (Some(idx), Some(start), Some(curr)) = (self.dragged_element_idx, self.drag_start_pos, self.current_drag_pos) {
            Some((idx, Vec2::new(curr.x - start.x, curr.y - start.y)))
        } else {
            None
        };

        let live_tip = match self.canvas.active_element() {
            Some(DrawingElement::Stroke(s)) => {
                s.live_tip.map(|(from, to)| (from, to, s.color, s.base_width, false))
            }
            Some(DrawingElement::Highlighter(s)) => {
                s.live_tip.map(|(from, to)| (from, to, s.color, s.base_width, true))
            }
            _ => None,
        };

        let active_shape = match self.canvas.active_element() {
            Some(shape @ (DrawingElement::Line { .. } | DrawingElement::Rect { .. } | DrawingElement::Ellipse { .. })) => Some(shape),
            _ => None,
        };

        let scene = Scene {
            elements: self.canvas.elements(),
            dock: &self.dock,
            is_overlay_active: self.is_active,
            hovered_idx: self.hovered_element_idx,
            is_eraser_hover: is_eraser,
            hovered_dock_tool: self.hovered_dock_tool,
            needs_full_rebuild: self.canvas.needs_full_rebuild,
            new_segments: &self.accumulated_unbaked_segments,
            live_tip,
            active_shape,
            live_drag_delta,
        };

        self.renderer.render(&scene, buffer, self.width, self.height);

        self.accumulated_unbaked_segments.clear();
        self.canvas.needs_full_rebuild = false;
    }
}
