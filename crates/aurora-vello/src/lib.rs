// Single responsibility: Vello GPU rasterization backend for the Aurora UI engine.
//
// Depends on `aurora` for the display list, geometry, and typography types; implements
// the GPU presentation backend.

pub mod command;
pub mod context;
pub mod gpu_backend;
pub mod path;
pub mod renderer;
pub mod shader;
pub mod shadow;
pub mod svg;
pub mod svg_paint;
pub mod text;

pub use gpu_backend::GpuBackend;
pub use renderer::VelloRenderer;

use std::sync::Arc;
use winit::window::Window;

/// Builds a boxed GPU presentation backend for [`aurora::app::AppConfig`].
///
/// Matches [`aurora::app::runner::BackendFactory`], so it can be passed straight to
/// [`aurora::app::AppConfig::backend_factory`].
pub fn create_backend(
    window: Arc<Window>,
    width: u32,
    height: u32,
    text_ctx: aurora::text::TextContext,
) -> Result<Box<dyn aurora::app::runner::Backend>, aurora::render::BackendError> {
    Ok(Box::new(GpuBackend::new(
        window, width, height, text_ctx,
    )?))
}

/// Builds a boxed CPU-staged Vello presentation backend for [`aurora::app::AppConfig`].
///
/// Same contract as [`create_backend`], but coarse rasterization runs on the
/// CPU (`RendererOptions::use_cpu`). Pair with `RenderBackend::VelloCpu`.
pub fn create_cpu_backend(
    window: Arc<Window>,
    width: u32,
    height: u32,
    text_ctx: aurora::text::TextContext,
) -> Result<Box<dyn aurora::app::runner::Backend>, aurora::render::BackendError> {
    Ok(Box::new(GpuBackend::new_cpu(
        window, width, height, text_ctx,
    )?))
}
