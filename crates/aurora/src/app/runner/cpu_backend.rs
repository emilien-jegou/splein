// Single responsibility: TinySkia CPU backend owning its softbuffer surface and rasterizer.

use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::app::presenter::SurfacePresenter;
use crate::app::runner::backend::{Backend, Present, PresentReport};
use crate::render::{BackendError, TinySkiaRenderer};
use crate::text::TextContext;

/// CPU SIMD rasterizer paired with softbuffer OS window presentation.
pub struct CpuBackend {
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: TinySkiaRenderer,
}

impl CpuBackend {
    /// Builds a CPU backend sized to the window with the given typography context.
    pub fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        let context = Context::new(window.clone())
            .map_err(|e| BackendError::init(format!("softbuffer context: {e:?}")))?;
        let mut surface = Surface::new(&context, window)
            .map_err(|e| BackendError::init(format!("softbuffer surface: {e:?}")))?;
        let (w, h) = (width.max(1), height.max(1));
        let _ = surface.resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        let mut renderer = TinySkiaRenderer::new();
        renderer.set_text_context(text_ctx);
        renderer.resize(width, height);
        Ok(Self {
            _context: context,
            surface,
            renderer,
        })
    }
}

impl Backend for CpuBackend {
    fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        let _ = self
            .surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        self.renderer.resize(width, height);
    }

    fn tag(&self) -> &'static str {
        "CPU"
    }

    fn supports_scanline(&self) -> bool {
        true
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        let t_raster = Instant::now();
        self.renderer.render_damage(frame.scene, frame.damage)?;
        let raster = t_raster.elapsed();

        let t_present = Instant::now();
        let buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| BackendError::surface(format!("buffer acquire: {e:?}")))?;
        let accumulated = frame.damage_resolver.for_age(buffer.age());
        let (width, height) = (frame.surface.width, frame.surface.height);
        let mut post = frame.post_process;

        SurfacePresenter::present_damaged()
            .pixmap(self.renderer.canvas())
            .buffer(buffer)
            .damage(&accumulated)
            .extra_damage(frame.extra_damage)
            .active_w(width)
            .active_h(height)
            .post_process(move |dst, dmg, stride, h| {
                if let Some(pass) = post.as_mut() {
                    pass.apply(dst, dmg, stride, h);
                }
            })
            .call();

        Ok(PresentReport {
            raster,
            present: t_present.elapsed(),
        })
    }
}
