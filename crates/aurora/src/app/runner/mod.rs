// Single responsibility: Winit event loop lifecycle coordination and late-latch frame pacing.

mod adapters;
pub mod backend;
pub mod clipboard;
pub(crate) mod frame_flow;
pub mod pacing;
#[cfg(feature = "desktop")]
pub mod pointer;

pub use backend::{
    Backend, DamageResolver, FramePostProcess, Present, PresentReport, QuadEngineStat,
    SurfaceRequest,
};
#[cfg(feature = "desktop")]
pub use backend::BackendFactory;
pub use pacing::FpsLimit;

#[cfg(feature = "desktop")]
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(feature = "desktop")]
use winit::event::{ElementState, Event, WindowEvent};
#[cfg(feature = "desktop")]
use winit::event_loop::{ControlFlow, EventLoop};
#[cfg(feature = "desktop")]
use winit::keyboard::NamedKey;
#[cfg(feature = "desktop")]
use winit::window::Window;

#[cfg(feature = "desktop")]
use crate::app::extension::AppExtension;
#[cfg(feature = "desktop")]
use crate::input::CursorIcon;
#[cfg(feature = "desktop")]
use crate::runtime::Engine;
#[cfg(feature = "desktop")]
use clipboard::OsClipboardSync;
#[cfg(feature = "desktop")]
use frame_flow::present_one;
#[cfg(feature = "desktop")]
use pacing::{compute_jit_wake, compute_target_interval};
#[cfg(feature = "desktop")]
use pointer::PointerBridge;
/// Runs the native window event loop, driving frame updates and presentation.
#[cfg(feature = "desktop")]
#[tracing::instrument(skip_all, fields(fps_limit = ?fps_limit))]
pub fn run_event_loop(
    event_loop: EventLoop<()>,
    window: Arc<Window>,
    mut engine: Engine,
    mut backend: Box<dyn Backend>,
    mut extensions: Vec<Box<dyn AppExtension>>,
    fps_limit: FpsLimit,
) {
    let mut pending_resize: Option<(u32, u32)> = None;
    let mut last_activity = Instant::now();
    let mut last_present = Instant::now();
    let mut redraw_pending = true;
    let mut avg_compute = Duration::from_millis(2);
    let mut pointer = PointerBridge::new();
    let mut cursor = CursorIcon::Default;
    // Copy-out only: readonly Ctrl+C selections reach the OS clipboard here,
    // in a real window only. Headless engines never construct this path.
    let mut os_clipboard = OsClipboardSync::new();

    let target_interval = compute_target_interval(fps_limit, &window);
    window.request_redraw();

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent {
                event: win_event, ..
            } => {
                last_activity = Instant::now();
                // Input is forwarded before the extension hook so a demo's own handler and
                // the engine's hit testing see events in the order they happened.
                if pointer.dispatch(&win_event, &mut engine) {
                    redraw_pending = true;
                }
                // The I-beam follows hover over selectable text; syncing only on
                // change keeps the platform call out of the hot path.
                let icon = engine.cursor_icon();
                if icon != cursor {
                    cursor = icon;
                    window.set_cursor_icon(match icon {
                        CursorIcon::Default => winit::window::CursorIcon::Default,
                        CursorIcon::Text => winit::window::CursorIcon::Text,
                    });
                }
                if extensions.iter_mut().any(|ext| ext.on_event(&win_event)) {
                    redraw_pending = true;
                }
                match win_event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::KeyboardInput { event: key, .. }
                        if key.state == ElementState::Pressed
                            && key.logical_key == winit::keyboard::Key::Named(NamedKey::Escape) =>
                    {
                        // Demos exit on Esc: the gallery launcher relies on this to
                        // get back to the menu when a child demo closes itself.
                        elwt.exit()
                    }
                    WindowEvent::Resized(size) => {
                        pending_resize = Some((size.width.max(1), size.height.max(1)));
                        redraw_pending = true;
                    }
                    WindowEvent::RedrawRequested => {
                        let mut wants_redraw = false;
                        for ext in extensions.iter_mut() {
                            wants_redraw |= ext.on_update(&mut engine);
                        }
                        let compute_dur = present_one(
                            &mut engine,
                            &mut *backend,
                            &mut extensions,
                            pending_resize.take(),
                        );
                        avg_compute = avg_compute.mul_f32(0.8) + compute_dur.mul_f32(0.2);
                        last_present = Instant::now();
                        redraw_pending = wants_redraw || engine.has_active_motion();
                        // Key events dispatched inside this frame may have set
                        // the internal clipboard (Ctrl+C): push on change,
                        // fail-soft when there is no OS clipboard to take it.
                        os_clipboard.sync(&engine);
                    }
                    _ => {}
                }
            }
            Event::UserEvent(()) => {
                last_activity = Instant::now();
                redraw_pending = true;
            }
            Event::AboutToWait => handle_idle_pacing(
                redraw_pending,
                &window,
                elwt,
                target_interval,
                last_present,
                avg_compute,
                last_activity,
                &mut extensions,
            ),
            Event::Resumed => {
                redraw_pending = true;
                window.request_redraw();
            }
            _ => {}
        })
        .expect("Event loop execution failed");
}

#[cfg(feature = "desktop")]
fn handle_idle_pacing(
    pending: bool,
    window: &Window,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
    interval: Option<Duration>,
    last_present: Instant,
    avg_compute: Duration,
    last_act: Instant,
    extensions: &mut [Box<dyn AppExtension>],
) {
    if pending {
        if let Some(i) = interval {
            let wake = compute_jit_wake(last_present, i, avg_compute);
            if Instant::now() >= wake {
                window.request_redraw();
            } else {
                elwt.set_control_flow(ControlFlow::WaitUntil(wake));
            }
        } else {
            window.request_redraw();
        }
    } else if last_act.elapsed() >= Duration::from_millis(300) {
        if extensions.iter_mut().any(|e| e.on_idle()) {
            window.request_redraw();
        } else {
            elwt.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(500),
            ));
        }
    } else {
        elwt.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(500),
        ));
    }
}

/// Graceful display-init failure shared by every app entry point.
///
/// A bare `.expect` here once ate a whole verification session in a display-less
/// sandbox behind a one-line message. Name the cause, point at headless
/// verification, and exit non-zero instead of panicking.
#[cfg(feature = "desktop")]
pub fn fail_display_init<T>(err: winit::error::EventLoopError) -> T {
    eprintln!("aurora: display init failed ({err})");
    eprintln!(
        "aurora: no X11/Wayland display reachable from this environment? \
         Verify headlessly instead: aurora selftest --ui <image>."
    );
    std::process::exit(1);
}
