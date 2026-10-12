// Single responsibility: Backend trait impl, present logic, damage culling.

use aurora::app::runner::backend::{Backend, Present, PresentReport};
use aurora::render::BackendError;
use aurora::text::TextContext;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::context::WgpuContext;
use crate::renderer::WgpuRenderer;

/// Raw wgpu backend with analytical shaders.
pub struct WgpuBackend {
    window: Arc<winit::window::Window>,
    renderer: WgpuRenderer,
    needs_present: bool,
}

impl WgpuBackend {
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        let gpu = pollster::block_on(WgpuContext::init(Arc::clone(&window), width, height))
            .map_err(BackendError::init)?;
        let mut renderer = WgpuRenderer::new(gpu, text_ctx);
        renderer.set_scale(window.scale_factor() as f32);
        Ok(Self {
            window,
            renderer,
            needs_present: true,
        })
    }
}

impl Backend for WgpuBackend {
    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        self.needs_present = true;
    }

    fn tag(&self) -> &'static str {
        "wgpu"
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        if frame.damage.is_empty() && frame.extra_damage.is_empty() && !self.needs_present {
            return Ok(PresentReport {
                raster: Duration::ZERO,
                present: Duration::ZERO,
            });
        }
        let t = Instant::now();
        // Scale can change without resize (monitor move); refresh per frame.
        self.renderer.set_scale(self.window.scale_factor() as f32);
        let mut cull = aurora::foundation::DamageRegion::new();
        for r in frame.damage.rects().iter().chain(frame.extra_damage.iter()) {
            cull.push(*r);
        }
        // No scroll blit on this backend yet: repaint the whole viewport the
        // shift moved through, from the new scene. Correct at full cost; the
        // blit fast path lands with the placement work.
        for shift in frame.damage.scroll_shifts() {
            cull.push(shift.clip);
        }
        self.renderer
            .render_frame(frame.scene, frame.surface.background, &cull)?;
        self.needs_present = false;
        Ok(PresentReport {
            raster: t.elapsed(),
            present: Duration::ZERO,
        })
    }
}
