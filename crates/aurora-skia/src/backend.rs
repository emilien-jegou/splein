// Single responsibility: Skia-slot CPU backend (softbuffer present).

use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use aurora::app::runner::backend::{Backend, Present, PresentReport};
use aurora::render::BackendError;
use aurora::text::TextContext;
use aurora_tiny_skia::{presenter::SurfacePresenter, TinySkiaRenderer};

pub struct SkiaBackend {
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: TinySkiaRenderer,
    resize_pending: bool,
}

impl SkiaBackend {
    pub fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        let context = Context::new(window.clone())
            .map_err(|e| BackendError::init(format!("skia softbuffer context: {e:?}")))?;
        let mut surface = Surface::new(&context, window)
            .map_err(|e| BackendError::init(format!("skia softbuffer surface: {e:?}")))?;
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

impl Backend for SkiaBackend {
    fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        let _ = self
            .surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        self.renderer.resize(width, height);
        self.resize_pending = true;
    }

    fn tag(&self) -> &'static str {
        "skia"
    }

    fn supports_scanline(&self) -> bool {
        true
    }

    fn present(&mut self, mut frame: Present<'_>) -> Result<PresentReport, BackendError> {
        let t_raster = Instant::now();
        let mut raster_damage = frame.damage.clone();
        for rect in frame.extra_damage {
            raster_damage.push(*rect);
        }
        self.renderer.render_damage(frame.scene, &raster_damage)?;
        let raster = t_raster.elapsed();

        let t_present = Instant::now();
        let buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| BackendError::surface(format!("skia buffer acquire: {e:?}")))?;
        let (width, height) = (frame.surface.width, frame.surface.height);
        let mut post = frame.post_process.take();
        if self.resize_pending {
            self.resize_pending = false;
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
        let accumulated = frame.damage_resolver.for_age(buffer.age());
        let mut os_damage = accumulated;
        for shift in frame.damage.scroll_shifts() {
            os_damage.push(shift.clip);
        }
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
