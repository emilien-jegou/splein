// Single responsibility: Library root exporting geometry, layout, tree, text, reactive, scene, render, and DSL domains.

pub use app::{App, AppConfig, FpsLimit, SurfacePresenter};
pub use dsl::*;
pub use foundation::*;
pub use layout::layout_node;
pub use pipeline::*;
pub use reactive::{batch, Derived, Effect, ReactiveRuntime, Signal};
pub use render::{RenderBackend, TinySkiaRenderer};
#[cfg(feature = "vello")]
pub use render::VelloRenderer;
pub use runtime::{Engine, FrameScheduler, FrameStats};
pub use scene::*;
pub use tree::*;

pub mod app;
pub mod dsl;
pub mod foundation;
pub mod layout;
pub mod pipeline;
pub mod prelude;
pub mod reactive;
pub mod render;
pub mod runtime;
pub mod scene;
pub mod text;
pub mod tree;
