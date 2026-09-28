// crates/splein/src/platform/wayland/mod.rs

pub mod cursor;
pub mod surface;

use crate::app::session::{OverlaySession, SessionAction};
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::Vec2;
use crate::platform::ipc::SOCKET_PATH;
use crate::render::icons::get_icons;
use crate::render::TinySkiaRenderer;
use calloop::generic::Generic;
use calloop::{Interest, Mode, PostAction};
use cursor::CursorManager;
use surface::OverlaySurface;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{LayerShell, LayerShellHandler, LayerSurface},
    shm::{Shm, ShmHandler},
};
use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::net::UnixListener;
use std::path::Path;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_region, wl_seat, wl_surface},
    Connection, EventQueue, QueueHandle, WEnum,
};

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

pub struct AppContext {
    pub conn: Connection,
    pub event_queue: EventQueue<WaylandAppState>,
    pub state: WaylandAppState,
}

pub fn run_overlay_daemon() -> eyre::Result<()> {
    let _ = get_icons();

    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let wayland_fd = conn.as_fd().try_clone_to_owned()?;

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm_state = Shm::bind(&globals, &qh)?;
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    // Bind initially to the active/primary output
    let initial_output = output_state.outputs().next();
    let overlay_surface = OverlaySurface::new(&compositor_state, &layer_shell, &shm_state, &qh, initial_output.as_ref())?;
    let cursor_manager = CursorManager::new(&compositor_state, &shm_state, &qh)?;

    let state = WaylandAppState {
        qh: qh.clone(),
        registry_state: RegistryState::new(&globals),
        seat_state,
        output_state,
        compositor_state,
        layer_shell,
        shm_state,
        session: OverlaySession::new(TinySkiaRenderer::new()),
        surface: overlay_surface,
        cursor: cursor_manager,
        cursor_pos: Vec2::ZERO,
        is_pointer_down: false,
    };

    let mut ctx = AppContext {
        conn,
        event_queue,
        state,
    };

    let mut event_loop = calloop::EventLoop::<AppContext>::try_new()?;
    let loop_handle = event_loop.handle();

    // 1. Epoll Wayland Connection FD
    let wayland_source = Generic::new(wayland_fd, Interest::READ, Mode::Level);
    loop_handle.insert_source(wayland_source, |_, _, ctx| {
        if let Some(guard) = ctx.conn.prepare_read() {
            let _ = guard.read();
        }
        let _ = ctx.event_queue.dispatch_pending(&mut ctx.state);
        let _ = ctx.conn.flush();
        Ok(PostAction::Continue)
    })?;

    // 2. Epoll Unix IPC Socket FD
    let sock_path = Path::new(SOCKET_PATH);
    if sock_path.exists() {
        let _ = std::fs::remove_file(sock_path);
    }
    let listener = UnixListener::bind(sock_path)?;
    listener.set_nonblocking(true)?;
    eprintln!("[splein] IPC socket listening on {}", SOCKET_PATH);

    let ipc_source = Generic::new(listener, Interest::READ, Mode::Level);
    loop_handle.insert_source(ipc_source, |_, listener, ctx| {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            if let Ok(n) = stream.read(&mut buf) {
                let cmd_str = String::from_utf8_lossy(&buf[..n]);
                let cmd = cmd_str.trim();

                if cmd.starts_with("toggle") {
                    let target_screen = cmd.strip_prefix("toggle").map(str::trim).filter(|s| !s.is_empty());
                    ctx.state.toggle_overlay(target_screen);
                    let _ = ctx.conn.flush();
                } else if cmd == "clear" {
                    let action = ctx.state.session.clear();
                    ctx.state.process_action(action);
                    let _ = ctx.conn.flush();
                } else if cmd == "undo" {
                    let action = ctx.state.session.undo();
                    ctx.state.process_action(action);
                    let _ = ctx.conn.flush();
                }
            }
        }
        Ok(PostAction::Continue)
    })?;

    let _ = ctx.conn.flush();
    eprintln!("[splein] Daemon running. Waiting for commands...");

    loop {
        event_loop.dispatch(None, &mut ctx)?;
        let _ = ctx.conn.flush();
    }
}

impl WaylandAppState {
    pub fn toggle_overlay(&mut self, target_screen: Option<&str>) {
        if !self.session.is_active() {
            let target_output = self.find_output(target_screen);
            let needs_rebind = match (&self.surface.target_output, &target_output) {
                (Some(current), Some(target)) => current != target,
                (None, Some(_)) => true,
                _ => false,
            };

            if needs_rebind {
                self.recreate_surface_on_output(target_output.as_ref());
            }
        }

        let action = self.session.toggle_active();
        self.process_action(action);
    }

    fn find_output(&self, screen_name: Option<&str>) -> Option<wl_output::WlOutput> {
        let mut first = None;
        for output in self.output_state.outputs() {
            if let Some(info) = self.output_state.info(&output) {
                if let Some(target) = screen_name {
                    if info.name.as_deref() == Some(target) {
                        return Some(output);
                    }
                }
                if first.is_none() {
                    first = Some(output);
                }
            }
        }
        first
    }

    fn recreate_surface_on_output(&mut self, output: Option<&wl_output::WlOutput>) {
        if let Ok(new_surface) = OverlaySurface::new(&self.compositor_state, &self.layer_shell, &self.shm_state, &self.qh, output) {
            self.surface = new_surface;
        }
    }

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
                self.surface.draw_frame(&qh, |buffer| {
                    session.render_to_buffer(buffer);
                });
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
        self.surface.request_redraw(&qh, |buffer| {
            session.render_to_buffer(buffer);
        });
    }

    pub fn recreate_surface(&mut self) {
        let target_output = self.surface.target_output.clone();
        self.recreate_surface_on_output(target_output.as_ref());
        self.surface.set_passthrough(!self.session.is_active(), &self.compositor_state, &self.qh);
        self.request_redraw();
    }
}

impl wayland_client::Dispatch<wl_region::WlRegion, ()> for WaylandAppState {
    fn event(_: &mut Self, _: &wl_region::WlRegion, _: wl_region::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl wayland_client::Dispatch<wl_keyboard::WlKeyboard, ()> for WaylandAppState {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event {
            if matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed)) {
                let action = match key {
                    1 => Some(state.session.toggle_active()),

                    2 => Some(state.session.select_color(0)),
                    3 => Some(state.session.select_color(1)),
                    4 => Some(state.session.select_color(2)),
                    5 => Some(state.session.select_color(3)),
                    6 => Some(state.session.select_color(4)),

                    16 => Some(state.session.select_tool(ActiveTool::Pointer)),
                    17 => Some(state.session.select_tool(ActiveTool::Pen)),
                    18 => Some(state.session.select_tool(ActiveTool::Highlighter)),
                    19 => Some(state.session.select_tool(ActiveTool::Text)),
                    20 => Some(state.session.select_tool(ActiveTool::SelectRegion)),
                    21 => Some(state.session.select_tool(ActiveTool::Line)),
                    22 => Some(state.session.select_tool(ActiveTool::Rect)),
                    23 => Some(state.session.select_tool(ActiveTool::Ellipse)),
                    24 => Some(state.session.select_tool(ActiveTool::Eraser)),

                    14 | 111 => Some(state.session.clear()),
                    44 => Some(state.session.undo()),
                    _ => None,
                };

                if let Some(act) = action {
                    state.process_action(act);
                }
            }
        }
    }
}

delegate_compositor!(WaylandAppState);
delegate_layer!(WaylandAppState);
delegate_output!(WaylandAppState);
delegate_shm!(WaylandAppState);
delegate_seat!(WaylandAppState);
delegate_pointer!(WaylandAppState);
delegate_registry!(WaylandAppState);

impl ProvidesRegistryState for WaylandAppState {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    registry_handlers![OutputState, SeatState];
}

impl CompositorHandler for WaylandAppState {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        let session = &mut self.session;
        self.surface.on_frame_callback(qh, |buffer| {
            session.render_to_buffer(buffer);
        });
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for WaylandAppState {
    fn output_state(&mut self) -> &mut OutputState { &mut self.output_state }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.recreate_surface();
    }
}

impl ShmHandler for WaylandAppState {
    fn shm_state(&mut self) -> &mut Shm { &mut self.shm_state }
}

impl LayerShellHandler for WaylandAppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.recreate_surface();
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        _: u32,
    ) {
        let (mut w, mut h) = configure.new_size;

        if w == 0 || h == 0 {
            if let Some(ref target) = self.surface.target_output {
                if let Some(info) = self.output_state.info(target) {
                    if let Some(mode) = info.modes.iter().find(|m| m.current).or_else(|| info.modes.first()) {
                        w = mode.dimensions.0 as u32;
                        h = mode.dimensions.1 as u32;
                    }
                }
            } else if let Some(output) = self.output_state.outputs().next() {
                if let Some(info) = self.output_state.info(&output) {
                    if let Some(mode) = info.modes.iter().find(|m| m.current).or_else(|| info.modes.first()) {
                        w = mode.dimensions.0 as u32;
                        h = mode.dimensions.1 as u32;
                    }
                }
            }
        }
        if w == 0 { w = 1920; }
        if h == 0 { h = 1080; }

        self.surface.is_configured = true;
        self.surface.set_dimensions(w, h, &self.shm_state);
        self.session.set_dimensions(self.surface.width, self.surface.height);

        let session = &mut self.session;
        let qh = self.qh.clone();
        self.surface.draw_frame(&qh, |buffer| {
            session.render_to_buffer(buffer);
        });
    }
}

impl SeatHandler for WaylandAppState {
    fn seat_state(&mut self) -> &mut SeatState { &mut self.seat_state }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            let _ = self.seat_state.get_pointer(qh, &seat);
        }
        if cap == Capability::Keyboard {
            seat.get_keyboard(qh, ());
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for WaylandAppState {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, pointer: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            self.cursor_pos = Vec2::new(event.position.0 as f32, event.position.1 as f32);
            let over_dock = self.session.is_point_over_dock(self.cursor_pos);

            match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.cursor.on_pointer_enter(pointer.clone(), serial, self.session.current_tool(), over_dock);
                }
                PointerEventKind::Press { button: 0x110, .. } => {
                    self.is_pointer_down = true;
                    let action = self.session.handle_pointer_down(self.cursor_pos, 1.0);
                    self.process_action(action);
                }
                PointerEventKind::Release { button: 0x110, .. } => {
                    self.is_pointer_down = false;
                    let action = self.session.handle_pointer_up();
                    self.process_action(action);
                }
                PointerEventKind::Press { button: 0x111 | 0x14b, .. } => {
                    let action = self.session.begin_temporary_erase(self.cursor_pos);
                    self.process_action(action);
                }
                PointerEventKind::Release { button: 0x111 | 0x14b, .. } => {
                    let action = self.session.end_temporary_erase();
                    self.process_action(action);
                }
                PointerEventKind::Motion { .. } => {
                    self.cursor.update_cursor(self.session.current_tool(), over_dock);
                    let action = self.session.handle_pointer_move(self.cursor_pos, 1.0, self.is_pointer_down);
                    self.process_action(action);
                }
                _ => {}
            }
        }
    }
}
