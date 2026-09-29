// Single responsibility: Vello GPU compute rasterizer coordinating swapchain surface frames.

use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect};
use vello::{AaConfig, AaSupport, RenderParams, Renderer as VelloEngine, RendererOptions, Scene as VelloScene};
use winit::window::Window;

use crate::foundation::{Color, DamageRegion};
use crate::render::backend::Renderer;
use crate::render::vello::command::{compile_vello_chunk, VelloImageCache};
use crate::render::vello::context::GpuContext;
use crate::render::vello::shader::to_vello_color;
use crate::render::vello::shadow::VelloShadowCache;
use crate::render::vello::svg::VelloSvgCache;
use crate::render::vello::text::VelloGlyphCache;
use crate::scene::scene::Scene;
use crate::text::TextContext;

/// High-throughput GPU rasterizer executing compute shaders over display lists.
pub struct VelloRenderer {
    _window: Arc<Window>,
    gpu: GpuContext,
    vello_engine: VelloEngine,
    vello_scene: VelloScene,
    text_ctx: TextContext,
    tx_stack: Vec<Affine>,
    shadow_cache: VelloShadowCache,
    svg_cache: VelloSvgCache,
    image_cache: VelloImageCache,
    glyph_cache: VelloGlyphCache,
    viewport_bound: KurboRect,
}

impl VelloRenderer {
    /// Creates a Vello compute renderer configured for the target OS window surface.
    pub fn new(window: Arc<Window>, width: u32, height: u32) -> Result<Self, String> {
        let gpu = pollster::block_on(GpuContext::init(Arc::clone(&window), width, height))?;
        let vello_engine = VelloEngine::new(
            &gpu.device,
            RendererOptions {
                surface_format: Some(gpu.config.format),
                use_cpu: false,
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
            },
        ).map_err(|e| format!("Failed to initialize Vello engine: {:?}", e))?;

        let viewport_bound = KurboRect::new(0.0, 0.0, width.max(1) as f64, height.max(1) as f64);
        Ok(Self {
            _window: window,
            gpu,
            vello_engine,
            vello_scene: VelloScene::new(),
            text_ctx: TextContext::new(),
            tx_stack: Vec::with_capacity(32),
            shadow_cache: VelloShadowCache::new(),
            svg_cache: VelloSvgCache::new(),
            image_cache: VelloImageCache::new(),
            glyph_cache: VelloGlyphCache::new(),
            viewport_bound,
        })
    }

    /// Updates the typography shaper and glyph database handle.
    pub fn set_text_context(&mut self, text_ctx: TextContext) { self.text_ctx = text_ctx; }

    /// Resizes the WGPU swapchain and presentation bounds.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        self.viewport_bound = KurboRect::new(0.0, 0.0, width.max(1) as f64, height.max(1) as f64);
    }

    /// Renders display list scene to the swapchain and presents.
    #[tracing::instrument(name = "Vello::RenderFrame", skip_all)]
    pub fn render_frame(&mut self, scene: &Scene, base_color: Color) -> Result<(), String> {
        self.vello_scene.reset();
        self.tx_stack.clear();

        for chunk in scene.chunks.iter() {
            compile_vello_chunk()
                .commands(&chunk.commands)
                .scene(&mut self.vello_scene)
                .bound(&self.viewport_bound)
                .tx_stack(&mut self.tx_stack)
                .shadow_cache(&mut self.shadow_cache)
                .svg_cache(&mut self.svg_cache)
                .image_cache(&mut self.image_cache)
                .glyph_cache(&mut self.glyph_cache)
                .text_ctx(&self.text_ctx)
                .call();
        }

        let surface_texture = match self.gpu.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
                self.gpu.surface.get_current_texture().map_err(|e| format!("Swapchain recovery failed: {:?}", e))?
            }
            Err(e) => return Err(format!("Swapchain error: {:?}", e)),
        };

        self.vello_engine.render_to_surface(
            &self.gpu.device,
            &self.gpu.queue,
            &self.vello_scene,
            &surface_texture,
            &RenderParams {
                base_color: to_vello_color(base_color, 1.0),
                width: self.gpu.config.width,
                height: self.gpu.config.height,
                antialiasing_method: AaConfig::Area,
            },
        ).map_err(|e| format!("Vello GPU render execution failed: {:?}", e))?;

        surface_texture.present();
        Ok(())
    }
}

impl Renderer for VelloRenderer {
    type Error = String;

    #[tracing::instrument(skip_all)]
    fn render(&mut self, scene: &Scene, _damage: &DamageRegion) -> Result<(), Self::Error> {
        self.render_frame(scene, Color::WHITE)
    }
}
