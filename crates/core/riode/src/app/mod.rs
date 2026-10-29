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
//! use riode::prelude::*;
//!
//! #[component]
//! fn Page() -> impl IntoElement {
//!     let open = use_signal(false);
//!     group()
//!         .width(800.0)
//!         .height(600.0)
//!         .material(Fill::solid(Color::WHITE))
//!         .children([text(open.map(|o| if o { "Open" } else { "Closed" }))])
//! }
//!
//! fn main() {
//!     App::config()
//!         .title("Hello")
//!         .size(800, 600)
//!         .font("Inter", include_bytes!("inter-var.ttf"))
//!         .background(Color::WHITE)
//!         .backend_factory(riode_engine_wgpu::create_backend)
//!         .run(Page);
//! }
//! ```
//!
//! # Rasterizer
//!
//! The engine itself rasterizes nothing — the factory handed to
//! `.backend_factory(..)` does that. Only `riode_engine_wgpu::create_backend`
//! exists; forgetting it panics with a message naming the missing factory (a
//! window with no painter is a programming error, not a runtime condition).
//!
//! # Custom per-frame work
//!
//! Implement [`AppExtension`] and pass it with [`AppConfig::extension`] to run code every
//! frame (physics drivers, telemetry, custom painters via [`OverlayPainter`]). See
//! `examples/moving.rs` (a DVD-screensaver physics driver) and `examples/showcase.rs`.

pub mod extension;
pub mod painter;
pub mod runner;
pub mod telemetry;

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
use crate::foundation::Color;
#[cfg(feature = "desktop")]
use crate::hot_reload::SharedFactory;
use crate::reactive::ReactiveRuntime;
#[cfg(feature = "desktop")]
use crate::reactive::Cx;
use crate::runtime::Engine;
#[cfg(feature = "desktop")]
use runner::{run_event_loop, Backend, BackendFactory};

/// Declarative builder for application window initialization and lifecycle execution.
pub struct AppConfig {
    title: String,
    width: u32,
    height: u32,
    fullscreen: bool,
    visible: bool,
    fonts: Vec<Vec<u8>>,
    background: Color,
    #[cfg(feature = "desktop")]
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
            fullscreen: false,
            visible: true,
            fonts: Vec::new(),
            background: Color::WHITE,
            #[cfg(feature = "desktop")]
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
    /// Borderless fullscreen on the current monitor (gallery launcher).
    pub fn fullscreen(mut self, on: bool) -> Self {
        self.fullscreen = on;
        self
    }
    /// Whether the window starts visible. The hot-reload host builds the
    /// first UI image before creating the window, so it opens once at final
    /// chrome — no wrong-sized frame ever shows.
    pub fn visible(mut self, on: bool) -> Self {
        self.visible = on;
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
    /// Supplies the constructor for the rasterizer.
    ///
    /// The engine crate implements no rasterizer, so the factory comes from the backend
    /// crate: `riode_engine_wgpu::create_backend`. Desktop-only.
    #[cfg(feature = "desktop")]
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
                .with_fullscreen(
                    self.fullscreen
                        .then(|| winit::window::Fullscreen::Borderless(None)),
                )
                .with_resizable(true)
                .with_visible(self.visible)
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
        //
        // The builder is kept as a shared factory for hot-reload: re-running it under the
        // same `"root"` scope rebuilds the tree with every signal, spring, and ticker
        // intact (see `crate::hot_reload`).
        let root_factory: SharedFactory = Rc::new(move || root_fn().into_element());
        let cx = Cx::new(Rc::clone(&runtime));
        // Generation-scoped like every full-tree build: the initial tree's registrations
        // tag with this generation, so the first reload's commit can sweep them instead of
        // running beside their twins. The window stays open across the engine creation
        // below so the initial reconcile (which re-runs builders for dynamic segments)
        // tags with the same generation — closing it first would orphan the two halves
        // under different generations and the commit would drop the live one.
        runtime.borrow_mut().begin_build_window();
        let root = cx.enter_keyed("root", || root_factory()).into_element();

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
        engine.set_root_factory(Rc::clone(&root_factory));
        // Closes the window opened around the factory above, committing the initial
        // generation. Older tagged registrations cannot exist on a fresh runtime, so this
        // commit keeps everything and only arms future reload sweeps.
        runtime.borrow_mut().end_build_window(true);
        for f in &self.fonts {
            engine.load_font(f);
        }
        engine.runtime().borrow_mut().set_on_dirty(move || {
            let _ = proxy.send_event(());
        });

        let factory = self.backend_factory.expect(
            "no rasterizer factory: pass `riode_engine_wgpu::create_backend` via \
             `AppConfig::backend_factory`",
        );
        let backend: Box<dyn Backend> = factory(Arc::clone(&window), pw, ph, engine.text_context())
            .unwrap_or_else(|e| panic!("backend init failed: {e:?}"));

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
