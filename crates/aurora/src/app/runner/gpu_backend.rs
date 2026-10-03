// Single responsibility: Vello GPU backend presenting frames through a WGPU surface.

use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::app::runner::backend::{Backend, PresentFrame};
use crate::render::{BackendError, VelloRenderer};
use crate::text::TextContext;

/// Direct GPU compute rasterizer powered by Vello and WGPU.
pub struct GpuBackend {
    renderer: VelloRenderer,
}

impl GpuBackend {
    /// Builds a GPU backend for the given window and typography context.
    pub fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        let mut renderer = VelloRenderer::new(window, width, height)?;
        renderer.set_text_context(text_ctx);
        Ok(Self { renderer })
    }
}

impl Backend for GpuBackend {
    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    fn tag(&self) -> &'static str {
        "GPU"
    }

    fn present(&mut self, frame: PresentFrame<'_>) -> Result<(), BackendError> {
        let t_raster = Instant::now();
        self.renderer.render_frame(frame.scene, frame.background)?;
        frame.diagnostics.timings.raster = t_raster.elapsed();
        Ok(())
    }
}
