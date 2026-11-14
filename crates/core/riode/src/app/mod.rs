// Responsibility: Composition root managing desktop window lifecycle, render surface initialization, and event loops.

pub mod config;
pub mod extension;
pub mod painter;
pub mod runner;
pub mod telemetry;

pub use config::{AppConfig, DEFAULT_FONT};
pub use extension::{AppExtension, OverlayCaps};
pub use painter::OverlayPainter;
pub use runner::FpsLimit;

use std::cell::RefCell;
use std::rc::Rc;
#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use winit::event_loop::EventLoopBuilder;
#[cfg(feature = "desktop")]
use winit::window::WindowBuilder;

#[cfg(feature = "desktop")]
use crate::dsl::component::IntoElement;
#[cfg(feature = "desktop")]
use crate::hot_reload::SharedFactory;
use crate::reactive::ReactiveRuntime;
#[cfg(feature = "desktop")]
use crate::reactive::Cx;
use crate::runtime::Engine;
#[cfg(feature = "desktop")]
use runner::{run_event_loop, Backend};

pub struct App;

impl App {
    pub fn config() -> AppConfig {
        AppConfig::new()
    }
}

impl AppConfig {
    #[cfg(feature = "desktop")]
    pub fn run<E: IntoElement, F: Fn() -> E + 'static>(mut self, root_fn: F) {
        let event_loop = EventLoopBuilder::with_user_event()
            .build()
            .unwrap_or_else(crate::app::runner::fail_display_init);
        let proxy = event_loop.create_proxy();
        let window = Arc::new(
            WindowBuilder::new()
                .with_title(&self.title)
                .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
                .with_fullscreen(self.fullscreen.then(|| winit::window::Fullscreen::Borderless(None)))
                .with_resizable(true)
                .with_visible(self.visible)
                .build(&event_loop)
                .unwrap(),
        );

        let runtime = self.runtime.take().unwrap_or_else(|| Rc::new(RefCell::new(ReactiveRuntime::new())));
        let root_factory: SharedFactory = Rc::new(move || root_fn().into_element());
        let cx = Cx::new(Rc::clone(&runtime));

        runtime.borrow_mut().begin_build_window();
        let root = cx.enter_keyed("root", || root_factory()).into_element();
        let physical = window.inner_size();
        let (pw, ph) = (physical.width.max(1), physical.height.max(1));

        let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, pw, ph);
        engine.set_background(self.background);
        engine.set_root_factory(Rc::clone(&root_factory));
        runtime.borrow_mut().end_build_window(true);

        // Load user fonts or fallback to embedded Inter
        if self.fonts.is_empty() {
            engine.load_font(DEFAULT_FONT);
        } else {
            for f in &self.fonts { engine.load_font(f); }
        }

        engine.runtime().borrow_mut().set_on_dirty(move || { let _ = proxy.send_event(()); });

        let factory = self.backend_factory.expect("pass `riode_engine_wgpu::create_backend` via `AppConfig::backend_factory`");
        let backend: Box<dyn Backend> = factory(Arc::clone(&window), pw, ph, engine.text_context())
            .unwrap_or_else(|e| panic!("backend init failed: {e:?}"));

        run_event_loop(event_loop, window, engine, backend, self.extensions, self.fps_limit);
    }
}
