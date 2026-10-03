// Single responsibility: Winit event loop lifecycle coordination and late-latch frame pacing.

mod adapters;
pub mod backend;
pub mod cpu_backend;
mod frame_flow;
#[cfg(feature = "vello")]
pub mod gpu_backend;
pub mod pacing;

pub use backend::{
    Backend, DamageResolver, FramePostProcess, Present, PresentReport, SurfaceRequest,
};
pub use cpu_backend::CpuBackend;
#[cfg(feature = "vello")]
pub use gpu_backend::GpuBackend;
pub use pacing::FpsLimit;

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

use crate::app::extension::AppExtension;
use crate::runtime::Engine;
use frame_flow::present_one;
use pacing::{compute_jit_wake, compute_target_interval};

/// Runs the native window event loop, driving frame updates and presentation.
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

    let target_interval = compute_target_interval(fps_limit, &window);
    window.request_redraw();

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent {
                event: win_event, ..
            } => {
                last_activity = Instant::now();
                if extensions.iter_mut().any(|ext| ext.on_event(&win_event)) {
                    redraw_pending = true;
                }
                match win_event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(size) => {
                        pending_resize = Some((size.width.max(1), size.height.max(1)));
                        redraw_pending = true;
                    }
                    WindowEvent::RedrawRequested => {
                        redraw_pending = false;
                        let compute_dur = present_one(
                            &mut engine,
                            &mut *backend,
                            &mut extensions,
                            pending_resize.take(),
                        );
                        avg_compute = avg_compute.mul_f32(0.8) + compute_dur.mul_f32(0.2);
                        last_present = Instant::now();
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
