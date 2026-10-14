// Single responsibility: Tiny-Skia CPU rasterization backend for the Aurora UI engine.
//
// Depends on `aurora` for the display list, geometry, and typography types; implements
// the CPU presentation backend plus the headless rasterizer and CPU test harness.

pub mod clip;
pub mod command;
pub mod cpu_backend;
pub mod image;
pub mod linebuffer;
pub mod path;
pub mod presenter;
pub mod raster;
pub mod renderer;
pub mod shader;
pub mod shadow;
pub mod stroke;
pub mod svg;
pub mod testing;
pub mod text;

pub use cpu_backend::CpuBackend;
pub use raster::{HeadlessFrame, HeadlessRaster};
pub use renderer::TinySkiaRenderer;
pub use text::{render_text, LineCompositeCache, TextRenderParams};
pub use linebuffer::{FramebufferSink, LineBufferBackend, LineSink};

use std::sync::Arc;
use winit::window::Window;

/// Builds a boxed CPU presentation backend for [`aurora::app::AppConfig`].
///
/// Matches [`aurora::app::runner::BackendFactory`], so it can be passed straight to
/// [`aurora::app::AppConfig::backend_factory`].
pub fn create_backend(
    window: Arc<Window>,
    width: u32,
    height: u32,
    text_ctx: aurora::text::TextContext,
) -> Result<Box<dyn aurora::app::runner::Backend>, aurora::render::BackendError> {
    Ok(Box::new(CpuBackend::new(window, width, height, text_ctx)?))
}
