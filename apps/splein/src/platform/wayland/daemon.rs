// Bootstraps the Wayland connection, IPC socket, and runs the calloop event loop.

use super::cursor::CursorManager;
use super::surface::OverlaySurface;
use super::WaylandAppState;
use crate::domain::geometry::Vec2;
use crate::platform::ipc::socket_path;
use crate::render::TinySkiaRenderer;
use calloop::generic::Generic;
use calloop::{Interest, Mode, PostAction};
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;
use smithay_client_toolkit::shm::Shm;
use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::net::UnixListener;
use wayland_client::globals::registry_queue_init;
use wayland_client::{Connection, EventQueue};

pub struct AppContext {
    pub conn: Connection,
    pub event_queue: EventQueue<WaylandAppState>,
    pub state: WaylandAppState,
}

/// Starts the long-lived overlay daemon with the overlay initially inactive.
pub fn run_overlay_daemon() -> eyre::Result<()> {
    run_overlay(false)
}

/// Runs the overlay in the foreground, active immediately, without needing a daemon.
pub fn run_standalone_overlay() -> eyre::Result<()> {
    run_overlay(true)
}

fn run_overlay(start_active: bool) -> eyre::Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();
    let wayland_fd = conn.as_fd().try_clone_to_owned()?;

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm_state = Shm::bind(&globals, &qh)?;
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    let initial_output = output_state.outputs().next();
    let surface = OverlaySurface::new(
        &compositor_state,
        &layer_shell,
        &shm_state,
        &qh,
        initial_output.as_ref(),
    )?;
    let cursor = CursorManager::new(&compositor_state, &shm_state, &qh)?;

    let state = WaylandAppState {
        qh: qh.clone(),
        registry_state: RegistryState::new(&globals),
        seat_state,
        output_state,
        compositor_state,
        layer_shell,
        shm_state,
        session: crate::app::session::OverlaySession::new(TinySkiaRenderer::default()),
        surface,
        cursor,
        cursor_pos: Vec2::ZERO,
        is_pointer_down: false,
    };

    let mut ctx = AppContext {
        conn,
        event_queue,
        state,
    };
    let mut loop_engine = calloop::EventLoop::<AppContext>::try_new()?;
    let loop_handle = loop_engine.handle();

    let wayland_source = Generic::new(wayland_fd, Interest::READ, Mode::Level);
    loop_handle.insert_source(wayland_source, |_, _, ctx| {
        if let Some(guard) = ctx.conn.prepare_read() {
            let _ = guard.read();
        }
        let _ = ctx.event_queue.dispatch_pending(&mut ctx.state);
        let _ = ctx.conn.flush();
        Ok(PostAction::Continue)
    })?;

    let sock_path = socket_path();
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;
    listener.set_nonblocking(true)?;
    tracing::info!("IPC listening on {:?}", sock_path);

    let ipc_source = Generic::new(listener, Interest::READ, Mode::Level);
    loop_handle.insert_source(ipc_source, |_, l, ctx| {
        if let Ok((mut stream, _)) = l.accept() {
            let mut buf = [0u8; 64];
            if let Ok(n) = stream.read(&mut buf) {
                let cmd = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                let action = if cmd.starts_with("toggle") {
                    Some(ctx.state.session.toggle_active())
                } else if cmd == "clear" {
                    Some(ctx.state.session.clear())
                } else if cmd == "undo" {
                    Some(ctx.state.session.undo())
                } else {
                    None
                };
                if let Some(act) = action {
                    ctx.state.process_action(act);
                }
                let _ = ctx.conn.flush();
            }
        }
        Ok(PostAction::Continue)
    })?;

    let _ = ctx.conn.flush();

    if start_active {
        let action = ctx.state.session.activate();
        ctx.state.process_action(action);
    }

    loop {
        loop_engine.dispatch(None, &mut ctx)?;
        let _ = ctx.conn.flush();
    }
}
