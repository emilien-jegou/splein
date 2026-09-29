// Holds the Wayland application state container and coordinates view actions.

pub mod cursor;
pub mod daemon;
pub mod handlers;
pub mod input;
pub mod surface;

use crate::app::session::{OverlaySession, SessionAction};
use crate::domain::geometry::Vec2;
use crate::render::TinySkiaRenderer;
use cursor::CursorManager;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::shm::Shm;
use surface::OverlaySurface;
use wayland_client::protocol::wl_output;
use wayland_client::QueueHandle;

pub struct WaylandAppState {
    pub qh: QueueHandle<Self>,
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub compositor_state: CompositorState,
    pub layer_shell: LayerShell,
    pub shm_state: Shm,
    pub session: OverlaySession<TinySkiaRenderer>,
    pub surface: OverlaySurface,
    pub cursor: CursorManager,
    pub cursor_pos: Vec2,
    pub is_pointer_down: bool,
}

impl WaylandAppState {
    pub fn process_action(&mut self, action: SessionAction) {
        match action {
            SessionAction::Redraw => self.request_redraw(),
            SessionAction::ChangeInputPassthrough(passthrough) => {
                self.surface.waiting_for_frame = false;
                self.surface.set_passthrough(passthrough, &self.compositor_state, &self.qh);
                if passthrough {
                    self.cursor.reset_to_default();
                } else {
                    let over_dock = self.session.is_point_over_dock(self.cursor_pos);
                    self.cursor.update_cursor(self.session.current_tool(), over_dock);
                }
                let session = &mut self.session;
                let qh = self.qh.clone();
                self.surface.draw_frame(&qh, |b| session.render_to_buffer(b));
            }
            SessionAction::ToolChanged(tool) => {
                let over_dock = self.session.is_point_over_dock(self.cursor_pos);
                self.cursor.update_cursor(tool, over_dock);
                self.request_redraw();
            }
        }
    }

    pub fn request_redraw(&mut self) {
        let session = &mut self.session;
        let qh = self.qh.clone();
        self.surface.request_redraw(&qh, |b| session.render_to_buffer(b));
    }

    pub fn find_output(&self, name: Option<&str>) -> Option<wl_output::WlOutput> {
        let mut first = None;
        for out in self.output_state.outputs() {
            if let Some(info) = self.output_state.info(&out) {
                if let Some(target) = name {
                    if info.name.as_deref() == Some(target) { return Some(out); }
                }
                if first.is_none() { first = Some(out); }
            }
        }
        first
    }

    pub fn recreate_surface(&mut self, output: Option<&wl_output::WlOutput>) {
        if let Ok(new_surf) = OverlaySurface::new(&self.compositor_state, &self.layer_shell, &self.shm_state, &self.qh, output) {
            self.surface = new_surf;
        }
    }
}
