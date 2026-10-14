// Single responsibility: Vello GPU backend presenting frames through a WGPU surface.

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::window::Window;

use riode::app::runner::backend::{Backend, Present, PresentReport};
use riode::render::BackendError;
use riode::text::TextContext;

use crate::renderer::VelloRenderer;

/// Direct GPU compute rasterizer powered by Vello and WGPU.
pub struct GpuBackend {
    renderer: VelloRenderer,
    /// Set on construction and resize: the swapchain has no valid contents yet,
    /// so the next present must render even with empty damage.
    needs_present: bool,
    tag: &'static str,
}

impl GpuBackend {
    /// Builds a GPU backend for the given window and typography context.
    pub fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        Self::new_inner(window, width, height, text_ctx, false)
    }

    /// Builds a CPU-staged Vello backend for the given window.
    ///
    /// Coarse rasterization runs on the CPU; presentation still goes through
    /// the wgpu swapchain like [`Self::new`].
    pub fn new_cpu(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        Self::new_inner(window, width, height, text_ctx, true)
    }

    fn new_inner(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
        use_cpu: bool,
    ) -> Result<Self, BackendError> {
        let mut renderer = if use_cpu {
            VelloRenderer::new_cpu(window, width, height)?
        } else {
            VelloRenderer::new(window, width, height)?
        };
        renderer.set_text_context(text_ctx);
        Ok(Self {
            renderer,
            needs_present: true,
            tag: if use_cpu { "vello-cpu" } else { "vello-gpu" },
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
        self.tag
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        if frame.damage.is_empty() && frame.extra_damage.is_empty() && !self.needs_present {
            // Nothing changed and the swapchain still shows the last frame: skip
            // the full scene rebuild and GPU submission entirely. Unlike SHM,
            // there is no buffer-age damage to replay here.
            return Ok(PresentReport {
                raster: Duration::ZERO,
                present: Duration::ZERO,
            });
        }
        let t_raster = Instant::now();
        // Union frame damage + extra damage into one culling region.
        let mut cull = riode::foundation::DamageRegion::new();
        for r in frame.damage.rects().iter().chain(frame.extra_damage.iter()) {
            cull.push(*r);
        }
        // No scroll blit on this backend yet: repaint the whole viewport the
        // shift moved through, from the new scene (same fallback as wgpu).
        for shift in frame.damage.scroll_shifts() {
            cull.push(shift.clip);
        }
        // Empty (unknown) → full render path inside renderer.
        let encoded = self
            .renderer
            .render_frame(frame.scene, frame.surface.background, &cull)?;
        tracing::debug!(
            encoded_chunks = encoded,
            reused_chunks = self.renderer.last_reused,
            blit_rects = self.renderer.last_blit_rects,
            blit_bytes = self.renderer.last_blit_bytes,
            total = frame.scene.chunks.len()
        );
        self.needs_present = false;
        Ok(PresentReport {
            raster: t_raster.elapsed(),
            present: Duration::ZERO,
        })
    }
}
