// Single responsibility: Application window runner, event loop integration, and lifecycle manager.

pub mod debug;
pub mod debug_draw;
pub mod debug_font;
pub mod debug_hud;
pub mod debug_layout;
pub mod debug_overlay;
pub mod debug_scene;
pub mod debug_toggle;
pub mod extension;
pub mod painter;
pub mod runner;
pub mod telemetry;

pub use debug::{InspectorMode, VisualDebugger};
pub use extension::{AppExtension, OverlayCaps};
pub use painter::OverlayPainter;
pub use runner::FpsLimit;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use winit::event_loop::EventLoopBuilder;
use winit::window::WindowBuilder;

use crate::dsl::component::IntoElement;
use aurora_foundation::Color;
use aurora_reactive::{Cx, ReactiveRuntime};
use crate::render::RenderBackend;
use crate::runtime::Engine;
use runner::{run_event_loop, Backend, BackendFactory};

/// Declarative builder for application window initialization and lifecycle execution.
pub struct AppConfig {
    title: String,
    width: u32,
    height: u32,
    fonts: Vec<Vec<u8>>,
    background: Color,
    backend: RenderBackend,
    backend_factory: Option<BackendFactory>,
    extensions: Vec<Box<dyn AppExtension>>,
    runtime: Option<Rc<RefCell<ReactiveRuntime>>>,
    fps_limit: FpsLimit,
}

/// Entrypoint for launching Aurora UI desktop applications.
pub struct App;

impl App {
    /// Constructs default window application configuration builder.
    pub fn config() -> AppConfig {
        AppConfig {
            title: "Aurora Application".into(),
            width: 800,
            height: 600,
            fonts: Vec::new(),
            background: Color::WHITE,
            backend: RenderBackend::default(),
            backend_factory: None,
            extensions: Vec::new(),
            runtime: None,
            fps_limit: FpsLimit::Auto,
        }
    }
}

impl AppConfig {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
    pub fn font(mut self, _name: &str, bytes: &[u8]) -> Self {
        self.fonts.push(bytes.to_vec());
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }
    pub fn backend(mut self, backend: RenderBackend) -> Self {
        self.backend = backend;
        self
    }
    /// Supplies the constructor for the selected backend.
    ///
    /// The engine crate implements no rasterizer, so the factory comes from a backend
    /// crate: `aurora_tiny_skia::create_backend`, `aurora_vello::create_backend`,
    /// or `aurora_vello::create_cpu_backend` (for `RenderBackend::VelloCpu`).
    pub fn backend_factory(mut self, factory: BackendFactory) -> Self {
        self.backend_factory = Some(factory);
        self
    }
    pub fn extension(mut self, ext: impl AppExtension + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }
    pub fn runtime(mut self, runtime: Rc<RefCell<ReactiveRuntime>>) -> Self {
        self.runtime = Some(runtime);
        self
    }
    pub fn fps_limit(mut self, limit: impl Into<FpsLimit>) -> Self {
        self.fps_limit = limit.into();
        self
    }

    pub fn run<E: IntoElement, F: Fn() -> E + 'static>(mut self, root_fn: F) {
        let event_loop = EventLoopBuilder::with_user_event()
            .build()
            .expect("Display init failed");
        let proxy = event_loop.create_proxy();
        let window = Arc::new(
            WindowBuilder::new()
                .with_title(&self.title)
                .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
                .with_resizable(true)
                .build(&event_loop)
                .unwrap(),
        );
        let runtime = self
            .runtime
            .take()
            .unwrap_or_else(|| Rc::new(RefCell::new(ReactiveRuntime::new())));

        // The root builder runs inside the hook scope, so a `#[component]` page can call the
        // `use_*` hooks with no arguments and land its signals on the runtime the engine is
        // about to drain.
        let cx = Cx::new(Rc::clone(&runtime));
        let root = cx.enter(root_fn).into_element();

        // The engine, the backend surface, and pointer input all run in
        // physical pixels (the viewport is later kept from `Resized`, which
        // winit also reports physically). Seeding the physical size here —
        // rather than waiting for the first `Resized`, which may arrive after
        // the first press — keeps input aimed from the very first frame on
        // HiDPI displays. At scale 1 this is a no-op.
        let physical = window.inner_size();
        let (pw, ph) = (physical.width.max(1), physical.height.max(1));

        let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, pw, ph);
        engine.set_background(self.background);
        for f in &self.fonts {
            engine.load_font(f);
        }
        engine.runtime().borrow_mut().set_on_dirty(move || {
            let _ = proxy.send_event(());
        });

        let backend: Box<dyn Backend> = match self.backend {
            RenderBackend::Vello | RenderBackend::VelloCpu => {
                let factory = self.backend_factory.expect(
                    "no Vello backend factory: pass `aurora_vello::create_backend` \
                     (or `aurora_vello::create_cpu_backend` for `RenderBackend::VelloCpu`) \
                     via `AppConfig::backend_factory`",
                );
                factory(
                    Arc::clone(&window),
                    pw,
                    ph,
                    engine.text_context(),
                )
                .expect("Vello backend init failed")
            }
            RenderBackend::TinySkia => {
                let factory = self.backend_factory.expect(
                    "no CPU backend factory: pass `aurora_tiny_skia::create_backend` \
                     via `AppConfig::backend_factory`",
                );
                factory(
                    window.clone(),
                    pw,
                    ph,
                    engine.text_context(),
                )
                .expect("CPU backend init failed")
            }
        };

        run_event_loop(
            event_loop,
            window,
            engine,
            backend,
            self.extensions,
            self.fps_limit,
        );
    }
}
