// Responsibility: Implements Backend trait, presentation timing, damage aggregation, and hardware scroll blits.

use riode::app::runner::backend::{Backend, Present, PresentReport};
use riode::render::BackendError;
use riode::text::TextContext;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::instrument;

use crate::context::WgpuContext;
use crate::renderer::WgpuRenderer;

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

    fn apply_scroll_shifts(&self, shifts: &[riode::foundation::ScrollShift]) {
        if shifts.is_empty() || self.renderer.clear_next {
            return;
        }
        let mut enc = self.renderer.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("riode-scroll-shift"),
        });
        let scale = self.renderer.scale;
        let sw = self.renderer.gpu.config.width as i32;
        let sh = self.renderer.gpu.config.height as i32;

        for shift in shifts {
            let dx = (shift.delta.x * scale).round() as i32;
            let dy = (shift.delta.y * scale).round() as i32;
            let clip_x = (shift.clip.x * scale).round() as i32;
            let clip_y = (shift.clip.y * scale).round() as i32;
            let clip_w = (shift.clip.width * scale).round() as i32;
            let clip_h = (shift.clip.height * scale).round() as i32;

            let (src_x, dst_x, copy_w) = if dx >= 0 {
                (clip_x, clip_x + dx, clip_w - dx)
            } else {
                (clip_x - dx, clip_x, clip_w + dx)
            };

            let (src_y, dst_y, copy_h) = if dy >= 0 {
                (clip_y, clip_y + dy, clip_h - dy)
            } else {
                (clip_y - dy, clip_y, clip_h + dy)
            };

            if copy_w > 0 && copy_h > 0
                && src_x >= 0 && src_y >= 0 && (src_x + copy_w) <= sw && (src_y + copy_h) <= sh
                && dst_x >= 0 && dst_y >= 0 && (dst_x + copy_w) <= sw && (dst_y + copy_h) <= sh
            {
                enc.copy_texture_to_texture(
                    wgpu::ImageCopyTexture {
                        texture: &self.renderer.gpu.persistent,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: src_x as u32, y: src_y as u32, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::ImageCopyTexture {
                        texture: &self.renderer.gpu.persistent,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: dst_x as u32, y: dst_y as u32, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d { width: copy_w as u32, height: copy_h as u32, depth_or_array_layers: 1 },
                );
            }
        }
        self.renderer.gpu.queue.submit(Some(enc.finish()));
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

    #[instrument(skip_all, fields(needs_present = %self.needs_present))]
    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        if frame.damage.is_empty() && frame.extra_damage.is_empty() && !self.needs_present {
            return Ok(PresentReport {
                raster: Duration::ZERO,
                present: Duration::ZERO,
            });
        }
        let t = Instant::now();
        self.renderer.set_scale(self.window.scale_factor() as f32);

        self.apply_scroll_shifts(frame.damage.scroll_shifts());

        let mut cull = riode::foundation::DamageRegion::new();
        for r in frame.damage.rects().iter().chain(frame.extra_damage.iter()) {
            cull.push(*r);
        }

        self.renderer.render_frame(frame.scene, frame.surface.background, &cull)?;
        self.needs_present = false;
        Ok(PresentReport {
            raster: t.elapsed(),
            present: Duration::ZERO,
        })
    }
}
