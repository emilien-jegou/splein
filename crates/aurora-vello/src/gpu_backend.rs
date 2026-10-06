// Single responsibility: Vello GPU backend presenting frames through a WGPU surface.

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::window::Window;

use aurora::app::runner::backend::{Backend, Present, PresentReport};
use aurora::render::BackendError;
use aurora::text::TextContext;

use crate::renderer::VelloRenderer;

/// Direct GPU compute rasterizer powered by Vello and WGPU.
pub struct GpuBackend {
    renderer: VelloRenderer,
    /// Set on construction and resize: the swapchain has no valid contents yet,
    /// so the next present must render even with empty damage.
    needs_present: bool,
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
        Ok(Self {
            renderer,
            needs_present: true,
        })
    }
}

impl Backend for GpuBackend {
    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        // Reconfigure invalidates swapchain buffers: present unconditionally next.
        self.needs_present = true;
    }

    fn tag(&self) -> &'static str {
        "GPU"
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        if frame.damage.is_empty()
            && frame.extra_damage.is_empty()
            && !self.needs_present
        {
            // Nothing changed and the swapchain still shows the last frame: skip
            // the full scene rebuild and GPU submission entirely. Unlike SHM,
            // there is no buffer-age damage to replay here.
            return Ok(PresentReport {
                raster: Duration::ZERO,
                present: Duration::ZERO,
            });
        }
        let t_raster = Instant::now();
        self.renderer
            .render_frame(frame.scene, frame.surface.background)?;
        self.needs_present = false;
        Ok(PresentReport {
            raster: t_raster.elapsed(),
            present: Duration::ZERO,
        })
    }
}
