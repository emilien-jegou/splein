// Dispatches Wayland pointer and keyboard input events to the application session.

use super::WaylandAppState;
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::Vec2;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_region};
use wayland_client::{Connection, QueueHandle, WEnum};

impl PointerHandler for WaylandAppState {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, p: &wl_pointer::WlPointer, evs: &[PointerEvent]) {
        for ev in evs {
            self.cursor_pos = Vec2::new(ev.position.0 as f32, ev.position.1 as f32);
            let over_dock = self.session.is_point_over_dock(self.cursor_pos);

            let act = match ev.kind {
                PointerEventKind::Enter { serial } => {
                    self.cursor.on_pointer_enter(p.clone(), serial, self.session.current_tool(), over_dock);
                    None
                }
                PointerEventKind::Press { button: 0x110, .. } => {
                    self.is_pointer_down = true;
                    Some(self.session.handle_pointer_down(self.cursor_pos, 1.0))
                }
                PointerEventKind::Release { button: 0x110, .. } => {
                    self.is_pointer_down = false;
                    Some(self.session.handle_pointer_up())
                }
                PointerEventKind::Press { button: 0x111 | 0x14b, .. } => Some(self.session.begin_temporary_erase(self.cursor_pos)),
                PointerEventKind::Release { button: 0x111 | 0x14b, .. } => Some(self.session.end_temporary_erase()),
                PointerEventKind::Motion { .. } => {
                    self.cursor.update_cursor(self.session.current_tool(), over_dock);
                    Some(self.session.handle_pointer_move(self.cursor_pos, 1.0, self.is_pointer_down))
                }
                _ => None,
            };
            if let Some(action) = act { self.process_action(action); }
        }
    }
}

impl wayland_client::Dispatch<wl_keyboard::WlKeyboard, ()> for WaylandAppState {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, ev: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_keyboard::Event::Key { key, state: ks, .. } = ev {
            if matches!(ks, WEnum::Value(wl_keyboard::KeyState::Pressed)) {
                let act = match key {
                    1 => Some(state.session.toggle_active()),
                    16 => Some(state.session.select_tool(ActiveTool::Pointer)),
                    17 => Some(state.session.select_tool(ActiveTool::Pen)),
                    18 => Some(state.session.select_tool(ActiveTool::Highlighter)),
                    19 => Some(state.session.select_tool(ActiveTool::Text)),
                    21 => Some(state.session.select_tool(ActiveTool::Rect)),
                    24 => Some(state.session.select_tool(ActiveTool::Eraser)),
                    14 | 111 => Some(state.session.delete_selection()),
                    44 => Some(state.session.undo()),
                    _ => None,
                };
                if let Some(a) = act { state.process_action(a); }
            }
        }
    }
}

impl wayland_client::Dispatch<wl_region::WlRegion, ()> for WaylandAppState {
    fn event(_: &mut Self, _: &wl_region::WlRegion, _: wl_region::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
