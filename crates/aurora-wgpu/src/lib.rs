// Single responsibility: Raw wgpu backend crate root and factory entry point.

pub mod backend;
pub mod cache;
pub mod clip;
pub mod command;
pub mod context;
pub mod image;
pub mod pipeline;
pub mod renderer;
pub mod shadow;
pub mod svg;
pub mod text;

pub use backend::WgpuBackend;

use aurora::render::BackendError;
use aurora::text::TextContext;
use std::sync::Arc;

/// Constructs a raw wgpu presentation backend for a window.
pub fn create_backend(
    window: Arc<winit::window::Window>,
    width: u32,
    height: u32,
    text_ctx: TextContext,
) -> Result<Box<dyn aurora::app::runner::backend::Backend>, BackendError> {
    Ok(Box::new(WgpuBackend::new(window, width, height, text_ctx)?))
}
