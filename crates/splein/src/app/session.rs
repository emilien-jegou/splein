// Orchestrates user sessions, event processing, and frame rendering.

use super::dock_controller::DockController;
use super::gesture::GestureEngine;
use crate::domain::canvas::{Canvas, Rgba};
use crate::domain::dock::{ActiveTool, Intent};
use crate::domain::geometry::Vec2;
use crate::render::{Frame, RenderEngine};

pub enum SessionAction {
    Redraw,
    ChangeInputPassthrough(bool),
    ToolChanged(ActiveTool),
}

pub struct OverlaySession<R: RenderEngine> {
    pub canvas: Canvas,
    pub dock: DockController,
    pub gestures: GestureEngine,
    renderer: R,
    is_active: bool,
    color: Rgba,
    width: u32,
    height: u32,
    hovered_elem: Option<usize>,
    pre_erase: Option<ActiveTool>,
}

impl<R: RenderEngine> OverlaySession<R> {
    pub fn new(renderer: R) -> Self {
        Self {
            canvas: Canvas::default(),
            dock: DockController::default(),
            gestures: GestureEngine::default(),
            renderer,
            is_active: false,
            color: Rgba::from_rgba8(0x57, 0xFA, 0x58, 255),
            width: 1920,
            height: 1080,
            hovered_elem: None,
            pre_erase: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }
    pub fn current_tool(&self) -> ActiveTool {
        self.dock.current_tool()
    }
    pub fn is_point_over_dock(&self, p: Vec2) -> bool {
        self.dock.is_over(p)
    }
    pub fn is_animating(&self) -> bool {
        self.dock.is_animating()
    }
    pub fn set_dimensions(&mut self, w: u32, h: u32) {
        self.width = w;
        self.height = h;
        self.dock.set_screen_width(w);
        self.canvas.needs_full_rebuild = true;
    }
    pub fn toggle_active(&mut self) -> SessionAction {
        self.is_active = !self.is_active;
        SessionAction::ChangeInputPassthrough(!self.is_active)
    }
    pub fn clear(&mut self) -> SessionAction {
        self.canvas.clear();
        SessionAction::Redraw
    }
    pub fn undo(&mut self) -> SessionAction {
        self.canvas.undo();
        SessionAction::Redraw
    }

    pub fn select_tool(&mut self, tool: ActiveTool) -> SessionAction {
        self.dock.active_tool = tool;
        SessionAction::ToolChanged(tool)
    }

    pub fn begin_temporary_erase(&mut self, pos: Vec2) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }
        if self.pre_erase.is_none() && self.current_tool() != ActiveTool::Eraser {
            self.pre_erase = Some(self.current_tool());
        }
        self.canvas.erase_at(pos, 18.0);
        self.select_tool(ActiveTool::Eraser)
    }

    pub fn end_temporary_erase(&mut self) -> SessionAction {
        let restored = self.pre_erase.take().unwrap_or(ActiveTool::Pen);
        self.select_tool(restored)
    }

    pub fn handle_pointer_down(&mut self, pos: Vec2, pressure: f32) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }
        if self.dock.is_over(pos) {
            if let Some(Intent::SelectTool(t)) = self.dock.on_down(pos) {
                return SessionAction::ToolChanged(t);
            }
            return SessionAction::Redraw;
        }
        self.gestures.on_down(
            pos,
            pressure,
            self.current_tool(),
            self.color,
            &mut self.canvas,
        );
        SessionAction::Redraw
    }

    pub fn handle_pointer_move(
        &mut self,
        pos: Vec2,
        pressure: f32,
        is_down: bool,
    ) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }
        if self.dock.is_dragging() || self.dock.is_over(pos) {
            self.dock.on_move(pos);
            return SessionAction::Redraw;
        }
        if !is_down {
            self.hovered_elem = self.canvas.hit_test(pos, 14.0);
            return SessionAction::Redraw;
        }
        self.gestures.on_move(
            pos,
            pressure,
            self.current_tool(),
            self.color,
            &mut self.canvas,
        );
        SessionAction::Redraw
    }

    pub fn handle_pointer_up(&mut self, pos: Vec2) -> SessionAction {
        if self.dock.is_dragging() || self.dock.is_over(pos) {
            self.dock.on_up();
            return SessionAction::Redraw;
        }
        self.gestures.on_up(&mut self.canvas);
        SessionAction::Redraw
    }

    pub fn tick_animation(&mut self, dt_secs: f32) -> bool {
        self.dock.tick(dt_secs)
    }

    pub fn render_to_buffer(&mut self, buffer: &mut [u8]) {
        let frame = Frame::assemble(
            &self.canvas,
            &self.dock.geometry,
            &self.dock.motion,
            self.current_tool(),
            self.dock.active_shape,
            self.dock.submenu_open,
            &self.gestures.unbaked_segments,
            self.gestures.live_drag_delta(),
            self.is_active,
            self.hovered_elem,
        );
        self.renderer
            .render(&frame, buffer, self.width, self.height);
        self.gestures.unbaked_segments.clear();
        self.canvas.needs_full_rebuild = false;
    }
}
