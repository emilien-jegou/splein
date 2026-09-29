// Single responsibility: Polymorphic presentation backend dispatch, resizing, and rendering.

use softbuffer::Context;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;

use crate::app::extension::AppExtension;
use crate::app::presenter::SurfacePresenter;
use crate::foundation::{Color, DamageRegion, ResolvedRect};
use crate::render::TinySkiaRenderer;
#[cfg(feature = "vello")]
use crate::render::VelloRenderer;
use crate::runtime::FrameDiagnostics;
use crate::scene::Scene;

/// Active presentation backend runtime instance preserving context lifetimes.
pub enum ActiveBackend {
    /// CPU SIMD rasterizer paired with softbuffer OS window presentation.
    TinySkia {
        /// Retained softbuffer display context.
        _context: Context<Arc<Window>>,
        /// TinySkia CPU rasterizer holding the persistent canvas.
        renderer: TinySkiaRenderer,
        /// Softbuffer presentation surface.
        surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    },
    /// Direct GPU compute rasterizer powered by Vello and WGPU.
    #[cfg(feature = "vello")]
    Vello {
        /// Retained Vello GPU compute renderer.
        renderer: VelloRenderer,
    },
}

impl ActiveBackend {
    /// Resizes swapchain and persistent rasterizer buffers.
    pub fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::TinySkia {
                renderer, surface, ..
            } => {
                let _ = surface.resize(
                    NonZeroU32::new(width).unwrap(),
                    NonZeroU32::new(height).unwrap(),
                );
                renderer.resize(width, height);
            }
            #[cfg(feature = "vello")]
            Self::Vello { renderer } => renderer.resize(width, height),
        }
    }

    /// Backend label tag identifier.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::TinySkia { .. } => "CPU",
            #[cfg(feature = "vello")]
            Self::Vello { .. } => "GPU",
        }
    }

    /// Renders the frame and presents through the active backend.
    pub fn present_frame(
        &mut self,
        scene: &Scene,
        damage: &DamageRegion,
        damage_age_fn: impl FnOnce(u8) -> DamageRegion,
        bg: Color,
        width: u32,
        height: u32,
        diag: &mut FrameDiagnostics,
        extensions: &mut [Box<dyn AppExtension>],
    ) {
        let t_raster_start = Instant::now();
        match self {
            #[cfg(feature = "vello")]
            Self::Vello { renderer } => {
                let _ = renderer.render_frame(scene, bg);
                diag.timings.raster = t_raster_start.elapsed();
            }
            Self::TinySkia {
                renderer, surface, ..
            } => {
                let _ = renderer.render_damage(scene, damage);
                diag.timings.raster = t_raster_start.elapsed();

                let t_present_start = Instant::now();
                if let Ok(buffer) = surface.buffer_mut() {
                    let accumulated_damage = damage_age_fn(buffer.age());
                    let extra: Vec<ResolvedRect> = extensions
                        .iter()
                        .filter_map(|e| e.overlay_damage())
                        .collect();

                    SurfacePresenter::present_damaged()
                        .pixmap(renderer.canvas())
                        .buffer(buffer)
                        .damage(&accumulated_damage)
                        .extra_damage(&extra)
                        .active_w(width)
                        .active_h(height)
                        .post_process(|dst, dmg, stride, h| {
                            for ext in extensions.iter_mut() {
                                ext.on_present(dst, dmg, stride, h);
                            }
                        })
                        .call();
                    diag.timings.present = t_present_start.elapsed();
                }
            }
        }
    }
}
