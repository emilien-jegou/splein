// Orchestrates user sessions, event processing, and frame rendering.

use super::dock_controller::DockController;
use super::gesture::GestureEngine;
use super::selection;
use crate::domain::canvas::{Canvas, Rgba};
use crate::domain::dock::{ActiveTool, Intent};
use crate::domain::geometry::{Aabb, Vec2};
use crate::render::{CanvasDynamics, DockInputs, Frame, RenderEngine};

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
    selected: Vec<usize>,
    erase_drag: bool,
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
            selected: Vec::new(),
            erase_drag: false,
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
    /// Activates the overlay and captures input.
    pub fn activate(&mut self) -> SessionAction {
        self.set_active(true)
    }
    /// Toggles the overlay and input capture.
    pub fn toggle_active(&mut self) -> SessionAction {
        self.set_active(!self.is_active)
    }
    /// Applies the active state and returns the input passthrough change.
    fn set_active(&mut self, active: bool) -> SessionAction {
        self.is_active = active;
        SessionAction::ChangeInputPassthrough(!active)
    }
    pub fn clear(&mut self) -> SessionAction {
        self.canvas.clear();
        self.selected.clear();
        SessionAction::Redraw
    }
    pub fn undo(&mut self) -> SessionAction {
        self.canvas.undo();
        self.selected.clear();
        SessionAction::Redraw
    }
    /// Removes all selected elements, or the whole canvas when nothing is selected.
    pub fn delete_selection(&mut self) -> SessionAction {
        if self.selected.is_empty() {
            return self.clear();
        }
        self.canvas.remove_indices(&std::mem::take(&mut self.selected));
        SessionAction::Redraw
    }

    pub fn select_tool(&mut self, tool: ActiveTool) -> SessionAction {
        self.dock.active_tool = tool;
        SessionAction::ToolChanged(tool)
    }

    /// Starts a right-button erase drag, stashing the tool to restore afterwards.
    pub fn begin_temporary_erase(&mut self, pos: Vec2) -> SessionAction {
        if !self.is_active {
            return SessionAction::Redraw;
        }
        self.erase_drag = true;
        self.selected.clear();
        if self.pre_erase.is_none() && self.current_tool() != ActiveTool::Eraser {
            self.pre_erase = Some(self.current_tool());
        }
        self.canvas.erase_at(pos, 18.0);
        self.select_tool(ActiveTool::Eraser)
    }

    /// Ends the right-button erase drag, restoring the stashed tool if any.
    pub fn end_temporary_erase(&mut self) -> SessionAction {
        self.erase_drag = false;
        match self.pre_erase.take() {
            Some(tool) => self.select_tool(tool),
            None => SessionAction::Redraw,
        }
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
        if self.current_tool() == ActiveTool::Pointer {
            self.selected = selection::on_pointer_press(
                &mut self.canvas,
                &mut self.gestures,
                &self.selected,
                pos,
            );
            return SessionAction::Redraw;
        }
        if self.current_tool() == ActiveTool::Eraser {
            self.selected.clear();
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
        let tool = self.current_tool();
        if tool == ActiveTool::Eraser && (is_down || self.erase_drag) {
            self.gestures.on_move(pos, pressure, tool, self.color, &mut self.canvas);
            self.hovered_elem = self.canvas.hit_test(pos, 14.0);
            return SessionAction::Redraw;
        }
        let element_drag = self.gestures.is_dragging();
        if self.dock.is_dragging() || (self.dock.is_over(pos) && !element_drag) {
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

    /// Completes both dock and canvas gestures on pointer release.
    pub fn handle_pointer_up(&mut self) -> SessionAction {
        self.dock.on_up();
        if let Some(captured) = self.gestures.on_up(&mut self.canvas) {
            self.selected = captured;
        }
        SessionAction::Redraw
    }

    pub fn tick_animation(&mut self, dt_secs: f32) -> bool {
        self.dock.tick(dt_secs)
    }

    /// Selection box for resize handles: resize target, else the selected group under the pointer.
    fn selection_box(&self, resize: Option<(&[usize], Aabb, Aabb)>) -> Option<Aabb> {
        if let Some((_, _, target)) = resize {
            return Some(target);
        }
        if self.current_tool() != ActiveTool::Pointer || self.selected.is_empty() {
            return None;
        }
        let mut rect = selection::group_rect(&self.canvas, &self.selected)?;
        if let Some((_, delta)) = self.gestures.move_preview() {
            rect.translate(delta);
        }
        Some(rect)
    }

    pub fn render_to_buffer(&mut self, buffer: &mut [u8]) {
        let resize = self.gestures.resize_preview();
        let frame = Frame::assemble(
            &self.canvas,
            CanvasDynamics {
                tool: self.current_tool(),
                segments: &self.gestures.unbaked_segments,
                drag: self.gestures.move_preview(),
                resize,
                selection: self.selection_box(resize),
                band: self.gestures.band_rect(),
                active: self.is_active,
                hovered: self.hovered_elem,
            },
            DockInputs {
                geometry: &self.dock.geometry,
                motion: &self.dock.motion,
                shape: self.dock.active_shape,
                submenu_open: self.dock.submenu_open,
            },
        );
        self.renderer
            .render(&frame, buffer, self.width, self.height);
        self.gestures.unbaked_segments.clear();
        self.canvas.needs_full_rebuild = false;
    }
}
