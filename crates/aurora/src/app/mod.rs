//! Application window runner: event loop, backends, and lifecycle.
//!
//! This is the composition root — the one place that knows about *everything*. [`App::config`]
//! builds an [`AppConfig`], you describe your UI as a component function, and [`AppConfig::run`]
//! wires the window, the engine, a raster backend, and the event loop together. Nothing else
//! in the crate imports this module; that one-way dependency is deliberate.
//!
//! A minimal desktop app looks like this:
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! #[component]
//! fn Page() -> impl IntoElement {
//!     let open = use_signal(false);
//!     group().width(800.0).height(600.0).fill(Color::WHITE).children([
//!         group()
//!             .width(200.0)
//!             .height(60.0)
//!             .fill(Color::hex(0x2563EB))
//!             .on_click(move |_| open.set(!open.get()))
//!             .children([text(open.map(|o| if o { "Open" } else { "Closed" }))]),
//!     ])
//! }
//!
//! fn main() {
//!     App::config()
//!         .title("Hello")
//!         .size(800, 600)
//!         .font("Inter", include_bytes!("inter-var.ttf"))
//!         .background(Color::WHITE)
//!         .backend(RenderBackend::TinySkia)
//!         .backend_factory(aurora_tiny_skia::create_backend)
//!         .run(Page);
//! }
//! ```
//!
//! # Choosing a backend
//!
//! The engine itself rasterizes nothing — a factory from a backend crate does that:
//!
//! | [`RenderBackend`] | Factory | When |
//! |---|---|---|
//! | `TinySkia` (default) | `aurora_tiny_skia::create_backend` | CPU rasterization; the sane default and what the test harness uses |
//! | `Vello` | `aurora_vello::create_backend` | GPU rasterization via wgpu |
//! | `VelloCpu` | `aurora_vello::create_cpu_backend` | Vello's CPU-staged pipeline, for comparing against GPU |
//!
//! Forgetting `.backend_factory(..)` panics with a message naming the missing factory —
//! that panic is intentional (a window with no painter is a programming error, not a
//! runtime condition).
//!
//! # Custom per-frame work
//!
//! Implement [`AppExtension`] and pass it with [`AppConfig::extension`] to run code every
//! frame (physics drivers, telemetry, custom painters via [`OverlayPainter`]). See
//! `examples/moving.rs` (a DVD-screensaver physics driver) and `examples/showcase.rs`.
//!
//! # Debugging a live app
//!
//! [`VisualDebugger`] (the `debug_*` modules) draws layout boxes, scene chunks, and a
//! performance HUD over your UI. The human-operated debug keys are `F8` (freeze the
//! clock), `.` (advance one 1/120 s quantum while frozen), `[`/`]` (slow motion),
//! and `s` (write presented pixels to `$AURORA_CAPTURE_DIR`).
//!
//! # Headless use
//!
//! Tests and benchmarks skip this module entirely: [`crate::runtime::Engine::headless`]
//! plus `aurora_tiny_skia::HeadlessRaster` (or the `Harness` in
//! `aurora_tiny_skia::testing`) drive the engine without a window. If you find yourself
//! reaching for `App` in a test, reach for `Harness` instead.

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
use crate::foundation::Color;
use crate::reactive::{Cx, ReactiveRuntime};
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
    pub fn background(mut self, color: impl Into<Color>) -> Self {
        self.background = color.into();
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

        // The root builder runs inside a *stable* hook scope (`"root"`), so a
        // `#[component]` page can call the `use_*` hooks with no arguments and
        // land its signals on the runtime the engine is about to drain — and
        // so those hooks recall across remounts. Stability cascades
        // positionally: every component under the root derives a reproducible
        // identity (see `Cx::enter_named`). One runtime hosts one app, so one
        // `"root"` key is enough; tests that need stability name their own.
        let cx = Cx::new(Rc::clone(&runtime));
        let root = cx.enter_keyed("root", root_fn).into_element();

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
            RenderBackend::Vello | RenderBackend::VelloCpu | RenderBackend::Wgpu => {
                let want = match self.backend {
                    RenderBackend::Vello => "aurora_vello::create_backend",
                    RenderBackend::VelloCpu => "aurora_vello::create_cpu_backend",
                    _ => "aurora_wgpu::create_backend",
                };
                let factory = self.backend_factory.expect(
                    "no GPU backend factory: pass the matching create_backend via \
                     `AppConfig::backend_factory`",
                );
                factory(
                    Arc::clone(&window),
                    pw,
                    ph,
                    engine.text_context(),
                )
                .unwrap_or_else(|e| panic!("{want} backend init failed: {e:?}"))
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
