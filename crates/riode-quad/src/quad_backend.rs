// Single responsibility: Single-window 2x2 combine backend — all four raster
// engines render the same scene into quadrants of one softbuffer window.
//
// One engine frame, four quarter-size rasters, one vsync. The old four-window
// combine serialized four vsync-blocking presents and re-framed + re-presented
// on every window's redraw (4 frames x 4 presents per tick): ~12fps in
// release. GPU quadrants shade into their persistent textures (same code as
// standalone) and are read back to CPU; tiny-skia shades its quadrant
// directly; everything composites into one pixmap with a legend strip.

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;
use winit::window::Window;

use riode::app::runner::backend::{Backend, Present, PresentReport, QuadEngineStat};
use riode::foundation::{Color, DamageRegion, Point, ResolvedRect};
use riode::render::swizzle_rgba_to_bgra;
use riode::render::BackendError;
use riode::scene::Scene;
use riode::text::{
    FontStyle, LineHeight, TextAlign, TextConfig, TextContext, TextDecoration, TextOverflow,
};
use riode_tiny_skia::{render_text, TextRenderParams, TinySkiaRenderer};
use riode_vello::VelloRenderer;
use riode_wgpu::context::WgpuContext;
use riode_wgpu::renderer::WgpuRenderer;

/// Height of the legend strip across the top of the quad window.
pub const QUAD_STRIP: u32 = 120;

/// Quadrant size for a quad window: two columns, strip + two rows.
pub fn quad_layout(full_w: u32, full_h: u32) -> (u32, u32) {
    (
        (full_w / 2).max(1),
        ((full_h.saturating_sub(QUAD_STRIP)) / 2).max(1),
    )
}

/// Window size for a demo that wants `qw x qh` quadrants.
pub fn quad_window(qw: u32, qh: u32) -> (u32, u32) {
    (qw * 2, qh * 2 + QUAD_STRIP)
}

const ENGINE_NAMES: [&str; 4] = ["tiny-skia", "vello-gpu", "vello-cpu", "wgpu"];
const ENGINE_COLORS: [(u8, u8, u8); 4] = [
    (0x38, 0xBD, 0xF8),
    (0x10, 0xB9, 0x81),
    (0xF5, 0x9E, 0x0B),
    (0xA8, 0x55, 0xF7),
];

struct EngineSample {
    raster: Duration,
    stamps: VecDeque<Instant>,
}

impl EngineSample {
    fn new() -> Self {
        Self {
            raster: Duration::ZERO,
            stamps: VecDeque::with_capacity(600),
        }
    }

    fn record(&mut self, raster: Duration) {
        let now = Instant::now();
        self.raster = raster;
        self.stamps.push_back(now);
        while let Some(front) = self.stamps.front() {
            if now.duration_since(*front) > Duration::from_secs(6) {
                self.stamps.pop_front();
            } else {
                break;
            }
        }
    }

    fn avg_fps(&self) -> f32 {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(5);
        let mut count = 0u32;
        let mut oldest: Option<Instant> = None;
        for t in &self.stamps {
            if *t >= cutoff {
                if oldest.is_none() {
                    oldest = Some(*t);
                }
                count += 1;
            }
        }
        match (count, oldest) {
            (0 | 1, _) => 0.0,
            (_, Some(o)) => {
                let dt = (now - o).as_secs_f32().max(0.5);
                (count as f32 / dt).min(9999.0)
            }
            _ => 0.0,
        }
    }
}

/// Single-window combine backend: four engines, four quadrants, one present.
pub struct QuadBackend {
    window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    full_w: u32,
    full_h: u32,
    qw: u32,
    qh: u32,
    skia: TinySkiaRenderer,
    vello_gpu: VelloRenderer,
    vello_cpu: VelloRenderer,
    wgpu: WgpuRenderer,
    composite: Pixmap,
    text_ctx: TextContext,
    samples: [EngineSample; 4],
    /// Last readback bytes per GPU engine, reused when damage is empty.
    cached_gpu: [Option<Vec<u8>>; 3],
    composite_stamps: VecDeque<Instant>,
}

impl QuadBackend {
    /// Builds the quad backend: host surface at full size, one quadrant
    /// renderer per engine at half size.
    pub fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        let (fw, fh) = (width.max(1), height.max(1));
        let (qw, qh) = quad_layout(fw, fh);
        let context = Context::new(Arc::clone(&window))
            .map_err(|e| BackendError::init(format!("softbuffer context: {e:?}")))?;
        let mut surface = Surface::new(&context, Arc::clone(&window))
            .map_err(|e| BackendError::init(format!("softbuffer surface: {e:?}")))?;
        let _ = surface.resize(NonZeroU32::new(fw).unwrap(), NonZeroU32::new(fh).unwrap());

        let mut skia = TinySkiaRenderer::new();
        skia.set_text_context(text_ctx.clone());
        skia.resize(qw, qh);

        // Headless GPU contexts: no window surface each. The platform
        // allows a single swapchain surface per window, so the three GPU
        // rasterizers shade into their persistent targets (same code as
        // standalone) and read back for CPU compositing; only the host
        // softbuffer surface presents.
        let mut vello_gpu = VelloRenderer::new_headless(qw, qh)
            .map_err(|e| BackendError::init(format!("quad vello-gpu: {e:?}")))?;
        vello_gpu.set_text_context(text_ctx.clone());
        let mut vello_cpu = VelloRenderer::new_cpu_headless(qw, qh)
            .map_err(|e| BackendError::init(format!("quad vello-cpu: {e:?}")))?;
        vello_cpu.set_text_context(text_ctx.clone());

        let gpu =
            pollster::block_on(WgpuContext::init_headless(qw, qh)).map_err(BackendError::init)?;
        let mut wgpu = WgpuRenderer::new(gpu, text_ctx.clone());
        wgpu.set_scale(window.scale_factor() as f32);

        let mut composite = Pixmap::new(fw, fh)
            .ok_or_else(|| BackendError::init(format!("quad composite {fw}x{fh} alloc failed")))?;
        composite.fill(tiny_skia::Color::TRANSPARENT);

        Ok(Self {
            window,
            _context: context,
            surface,
            full_w: fw,
            full_h: fh,
            qw,
            qh,
            skia,
            vello_gpu,
            vello_cpu,
            wgpu,
            composite,
            text_ctx,
            samples: [
                EngineSample::new(),
                EngineSample::new(),
                EngineSample::new(),
                EngineSample::new(),
            ],
            cached_gpu: [None, None, None],
            composite_stamps: VecDeque::with_capacity(600),
        })
    }

    /// Quadrant origin in full-window pixels.
    fn quad_origin(&self, idx: usize) -> (u32, u32) {
        let x = (idx % 2) as u32 * self.qw;
        let y = QUAD_STRIP + (idx / 2) as u32 * self.qh;
        (x, y)
    }

    /// Blits RGBA bytes into the composite with src-over onto the backdrop.
    ///
    /// `straight` selects un-premultiplied input (wgpu writes straight alpha;
    /// tiny-skia and vello are premultiplied). Opaque pixels dominate, so the
    /// flag only affects antialiased edges; wrong guesses show as fringes.
    fn blit_quadrant(&mut self, idx: usize, rgba: &[u8], straight: bool) {
        let (ox, oy) = self.quad_origin(idx);
        let data = self.composite.data_mut();
        let fw = self.full_w as usize;
        let qw = self.qw as usize;
        let qh = self.qh as usize;
        for y in 0..qh {
            let dst_row = ((oy as usize + y) * fw + ox as usize) * 4;
            let src_row = y * qw * 4;
            for x in 0..qw {
                let si = src_row + x * 4;
                let di = dst_row + x * 4;
                let (sr, sg, sb, sa) = (
                    rgba[si] as u32,
                    rgba[si + 1] as u32,
                    rgba[si + 2] as u32,
                    rgba[si + 3] as u32,
                );
                if sa == 255 {
                    data[di] = rgba[si];
                    data[di + 1] = rgba[si + 1];
                    data[di + 2] = rgba[si + 2];
                    data[di + 3] = 255;
                } else if sa == 0 {
                    // Transparent: backdrop shows through untouched.
                } else if straight {
                    let inv = 255 - sa;
                    data[di] = ((sr * sa + data[di] as u32 * inv) / 255) as u8;
                    data[di + 1] = ((sg * sa + data[di + 1] as u32 * inv) / 255) as u8;
                    data[di + 2] = ((sb * sa + data[di + 2] as u32 * inv) / 255) as u8;
                    data[di + 3] = 255;
                } else {
                    let inv = 255 - sa;
                    data[di] = (sr + data[di] as u32 * inv / 255) as u8;
                    data[di + 1] = (sg + data[di + 1] as u32 * inv / 255) as u8;
                    data[di + 2] = (sb + data[di + 2] as u32 * inv / 255) as u8;
                    data[di + 3] = 255;
                }
            }
        }
    }

    /// Fills a full-window rect in the composite (legend chips, dividers).
    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, rgb: (u8, u8, u8)) {
        let data = self.composite.data_mut();
        let fw = self.full_w as usize;
        let fh = self.full_h as usize;
        for yy in y..(y + h).min(fh as u32) {
            for xx in x..(x + w).min(fw as u32) {
                let i = (yy as usize * fw + xx as usize) * 4;
                data[i] = rgb.0;
                data[i + 1] = rgb.1;
                data[i + 2] = rgb.2;
                data[i + 3] = 255;
            }
        }
    }

    fn legend_text(&mut self, content: String, x: f32, y: f32, color: Color) {
        let cfg = TextConfig {
            content,
            font_id: None,
            family: None,
            style: FontStyle::Normal,
            size: 13.0,
            weight: 700,
            line_height: LineHeight::Absolute(17.0),
            letter_spacing: 0.0,
            align: TextAlign::Start,
            decoration: TextDecoration::NONE,
            overflow: TextOverflow::Clip,
            max_lines: None,
            color,
            ..TextConfig::default()
        };
        let layout = self
            .text_ctx
            .shape_config(&cfg, riode::foundation::Constraints::loose(1200.0, 20.0));
        let mut pix = self.composite.as_mut();
        render_text(
            &mut pix,
            &self.text_ctx,
            TextRenderParams {
                origin: Point::new(x, y),
                layout: &layout,
                // Legend content changes every frame (live fps/ms), so the
                // retained line cache would churn without ever hitting:
                // per-glyph path for a handful of short strings.
                layout_arc: None,
                color,
                opacity: 1.0,
                transform: tiny_skia::Transform::identity(),
                clip: None,
                line_cache: None,
            },
        );
    }

    /// Draws the legend strip: title + per-engine chip, name, avg fps, ms.
    fn draw_legend(&mut self) {
        self.fill_rect(0, 0, self.full_w, QUAD_STRIP, (0x0F, 0x17, 0x2A));
        self.fill_rect(0, QUAD_STRIP - 2, self.full_w, 2, (0x33, 0x41, 0x55));
        let composite_fps = {
            let now = Instant::now();
            let cutoff = now - Duration::from_secs(5);
            let mut n = 0u32;
            let mut oldest: Option<Instant> = None;
            for t in &self.composite_stamps {
                if *t >= cutoff {
                    if oldest.is_none() {
                        oldest = Some(*t);
                    }
                    n += 1;
                }
            }
            match (n, oldest) {
                (0 | 1, _) => 0.0,
                (_, Some(o)) => (n as f32 / (now - o).as_secs_f32().max(0.5)).min(9999.0),
                _ => 0.0,
            }
        };
        let title = format!(
            "QUAD COMBINE  ·  one scene, four rasterizers  ·  {composite_fps:.0} fps  ·  F5 demos  F7 engine  Esc exit"
        );
        self.legend_text(title, 16.0, 8.0, Color::hex(0xE2E8F0));
        for i in 0..4 {
            let y = 32.0 + i as f32 * 21.0;
            let (r, g, b) = ENGINE_COLORS[i];
            self.fill_rect(16, y as u32 + 3, 10, 10, (r, g, b));
            let ms = self.samples[i].raster.as_secs_f32() * 1000.0;
            let line = format!(
                "{}   {:>4.0} fps avg5   r {:>5.1} ms",
                ENGINE_NAMES[i],
                self.samples[i].avg_fps(),
                ms
            );
            self.legend_text(line, 34.0, y, Color::hex(0x94A3B8));
        }
        // Quadrant dividers.
        let div = (0x33, 0x41, 0x55);
        self.fill_rect(self.qw - 1, QUAD_STRIP, 2, self.full_h - QUAD_STRIP, div);
        self.fill_rect(0, QUAD_STRIP + self.qh - 1, self.full_w, 2, div);
    }

    fn render_quadrants(
        &mut self,
        scene: &Scene,
        background: Color,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        // 0: tiny-skia shades its quadrant directly (damage-tiled, retained).
        let t = Instant::now();
        self.skia.render_damage(scene, damage)?;
        let skia_px: Vec<u8> = self.skia.canvas().data().to_vec();
        self.samples[0].record(t.elapsed());

        // 1-2: vello shades fullscreen into persistent textures (as
        // standalone); read back for compositing.
        let t = Instant::now();
        self.vello_gpu
            .render_scene_offscreen(scene, background, damage)?;
        let gpu_px = self.vello_gpu.readback_rgba8()?;
        self.samples[1].record(t.elapsed());

        let t = Instant::now();
        self.vello_cpu
            .render_scene_offscreen(scene, background, damage)?;
        let cpu_px = self.vello_cpu.readback_rgba8()?;
        self.samples[2].record(t.elapsed());

        // 3: wgpu repaints damage into its retained target (as standalone);
        // idle frames reuse the last readback.
        let t = Instant::now();
        let wgpu_px = if damage.is_empty() {
            match &self.cached_gpu[2] {
                Some(px) => px.clone(),
                None => {
                    self.wgpu
                        .render_scene_offscreen(scene, background, damage)?;
                    let px = self.wgpu.readback_rgba8()?;
                    self.cached_gpu[2] = Some(px.clone());
                    px
                }
            }
        } else {
            self.wgpu
                .render_scene_offscreen(scene, background, damage)?;
            let px = self.wgpu.readback_rgba8()?;
            self.cached_gpu[2] = Some(px.clone());
            px
        };
        self.samples[3].record(t.elapsed());
        let _ = &self.cached_gpu[0..2];

        self.blit_quadrant(0, &skia_px, false);
        self.blit_quadrant(1, &gpu_px, false);
        self.blit_quadrant(2, &cpu_px, false);
        self.blit_quadrant(3, &wgpu_px, true);
        Ok(())
    }
}

impl Backend for QuadBackend {
    fn resize(&mut self, width: u32, height: u32) {
        let (fw, fh) = (width.max(1), height.max(1));
        if (fw, fh) == (self.full_w, self.full_h) {
            return;
        }
        self.full_w = fw;
        self.full_h = fh;
        let (qw, qh) = quad_layout(fw, fh);
        self.qw = qw;
        self.qh = qh;
        let _ = self
            .surface
            .resize(NonZeroU32::new(fw).unwrap(), NonZeroU32::new(fh).unwrap());
        self.skia.resize(qw, qh);
        self.vello_gpu.resize(qw, qh);
        self.vello_cpu.resize(qw, qh);
        self.wgpu.resize(qw, qh);
        self.cached_gpu = [None, None, None];
        if let Some(px) = Pixmap::new(fw, fh) {
            self.composite = px;
        }
    }

    fn tag(&self) -> &'static str {
        "quad"
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        let t_raster = Instant::now();
        // Retained canvases/targets are presented every quad frame, so
        // age-1 damage (last frame + this frame) is the correct repaint set;
        // extension overlay footprints ride along as extra damage.
        let mut damage = frame.damage_resolver.for_age(1);
        for r in frame.extra_damage {
            damage.push(*r);
        }
        if damage.is_empty() {
            // Fresh canvases still need one full paint (first frame / resize
            // races where the ring reports nothing yet).
            damage.push(ResolvedRect::new(0.0, 0.0, self.qw as f32, self.qh as f32));
        }
        self.wgpu.set_scale(self.window.scale_factor() as f32);
        self.render_quadrants(frame.scene, frame.surface.background, &damage)?;
        let raster = t_raster.elapsed();

        let t_present = Instant::now();
        self.draw_legend();
        self.composite_stamps.push_back(Instant::now());
        while let Some(front) = self.composite_stamps.front() {
            if front.elapsed() > Duration::from_secs(6) {
                self.composite_stamps.pop_front();
            } else {
                break;
            }
        }
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| BackendError::surface(format!("quad buffer acquire: {e:?}")))?;
        let src: &[u32] = bytemuck::cast_slice(self.composite.data());
        let dst = buffer.as_mut();
        let n = (self.full_w as usize * self.full_h as usize)
            .min(dst.len())
            .min(src.len());
        swizzle_rgba_to_bgra(&mut dst[..n], &src[..n]);
        if let Some(pass) = frame.post_process {
            let empty = DamageRegion::new();
            pass.apply(dst, &empty, self.full_w as usize, self.full_h as usize);
        }
        let _ = buffer.present();
        Ok(PresentReport {
            raster,
            present: t_present.elapsed(),
        })
    }

    fn quad_stats(&self) -> Option<Vec<QuadEngineStat>> {
        Some(
            (0..4)
                .map(|i| QuadEngineStat {
                    name: ENGINE_NAMES[i],
                    fps_avg5: self.samples[i].avg_fps(),
                    raster_ms: self.samples[i].raster.as_secs_f32() * 1000.0,
                })
                .collect(),
        )
    }
}

/// Builds a boxed quad-combine backend for a window. Matches
/// [`riode::app::runner::BackendFactory`].
pub fn create_backend(
    window: Arc<Window>,
    width: u32,
    height: u32,
    text_ctx: TextContext,
) -> Result<Box<dyn Backend>, BackendError> {
    Ok(Box::new(QuadBackend::new(window, width, height, text_ctx)?))
}
