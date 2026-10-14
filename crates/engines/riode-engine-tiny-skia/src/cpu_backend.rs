// Single responsibility: TinySkia CPU backend owning its softbuffer surface and rasterizer.

use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use riode::app::runner::backend::{Backend, Present, PresentReport};
use riode::render::BackendError;
use riode::text::TextContext;

use crate::presenter::SurfacePresenter;
use crate::renderer::TinySkiaRenderer;

/// CPU SIMD rasterizer paired with softbuffer OS window presentation.
pub struct CpuBackend {
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: TinySkiaRenderer,
    resize_pending: bool,
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
            resize_pending: true,
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
        self.resize_pending = true;
    }

    fn tag(&self) -> &'static str {
        "tiny-skia"
    }

    fn supports_scanline(&self) -> bool {
        true
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        let t_raster = Instant::now();
        // Extension overlays (e.g. the hot-reload progress bar) are recorded
        // into the scene but their damage arrives via `extra_damage`, outside
        // the engine's own region. Rasterize the union: every other backend
        // (vello, wgpu, quad) already does, and without it the presenter below
        // copies stale pixmap bytes for the strip so overlays never appear.
        let mut raster_damage = frame.damage.clone();
        for rect in frame.extra_damage {
            raster_damage.push(*rect);
        }
        self.renderer.render_damage(frame.scene, &raster_damage)?;
        let raster = t_raster.elapsed();

        if self.resize_pending {
            self.resize_pending = false;
            let t_present = Instant::now();
            let buffer = self
                .surface
                .buffer_mut()
                .map_err(|e| BackendError::surface(format!("buffer acquire: {e:?}")))?;
            let (width, height) = (frame.surface.width, frame.surface.height);
            let mut post = frame.post_process;
            SurfacePresenter::present_viewport()
                .pixmap(self.renderer.canvas())
                .buffer(buffer)
                .active_w(width)
                .active_h(height)
                .post_process(move |dst, dmg, stride, h| {
                    if let Some(pass) = post.as_mut() {
                        pass.apply(dst, dmg, stride, h);
                    }
                })
                .call();
            return Ok(PresentReport {
                raster,
                present: t_present.elapsed(),
            });
        }
        let t_present = Instant::now();
        let buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| BackendError::surface(format!("buffer acquire: {e:?}")))?;
        let accumulated = frame.damage_resolver.for_age(buffer.age());
        // The blit moved retained pixels inside each shift clip: those pixels
        // changed since the last commit too, so the compositor must upload
        // them alongside the aged damage (whose union already dropped the
        // shifts — they belong to this frame only).
        let mut os_damage = accumulated;
        for shift in frame.damage.scroll_shifts() {
            os_damage.push(shift.clip);
        }
        let (width, height) = (frame.surface.width, frame.surface.height);
        let mut post = frame.post_process;

        SurfacePresenter::present_damaged()
            .pixmap(self.renderer.canvas())
            .buffer(buffer)
            .damage(&os_damage)
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
