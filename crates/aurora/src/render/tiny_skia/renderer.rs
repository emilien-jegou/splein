// Single responsibility: Software rasterization execution of Scene chunks via tiny-skia.

use tiny_skia::{Pixmap, Transform as SkiaTransform};

use crate::foundation::DamageRegion;
use crate::render::error::BackendError;
use crate::render::tiny_skia::clip::ClipStack;
use crate::render::tiny_skia::command::{execute_commands, CommandContext};
use crate::render::tiny_skia::image::ImageCache;
use crate::render::tiny_skia::layer::LayerCompositor;
use crate::render::tiny_skia::shadow::ShadowRasterizer;
use crate::render::tiny_skia::svg::SvgCache;
use crate::scene::command::LayerId;
use crate::scene::scene::Scene;
use crate::text::TextContext;

/// Software rasterizer driving scene chunks into CPU pixel buffers.
pub struct TinySkiaRenderer {
    persistent_canvas: Pixmap,
    clip_stack: ClipStack,
    pub text_ctx: TextContext,
    svg_cache: SvgCache,
    image_cache: ImageCache,
    shadow_rasterizer: ShadowRasterizer,
    layer_compositor: LayerCompositor,
    tx_stack: Vec<SkiaTransform>,
    opacity_stack: Vec<f32>,
    scratch_pixmap: Option<Pixmap>,
    active_layers: Vec<(LayerId, Pixmap)>,
}

impl TinySkiaRenderer {
    /// Constructs a clean software rasterizer instance.
    pub fn new() -> Self {
        let mut canvas = Pixmap::new(1, 1).unwrap();
        canvas.fill(tiny_skia::Color::TRANSPARENT);
        Self {
            persistent_canvas: canvas,
            clip_stack: ClipStack::new(),
            text_ctx: TextContext::new(),
            svg_cache: SvgCache::new(),
            image_cache: ImageCache::new(),
            shadow_rasterizer: ShadowRasterizer::new(),
            layer_compositor: LayerCompositor::new(),
            tx_stack: Vec::with_capacity(32),
            opacity_stack: Vec::with_capacity(32),
            scratch_pixmap: None,
            active_layers: Vec::new(),
        }
    }

    /// Reallocates persistent canvas while preserving shared bounds.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (nw, nh) = (width.max(1), height.max(1));
        if self.persistent_canvas.width() == nw && self.persistent_canvas.height() == nh {
            return;
        }
        let (ow, oh) = (
            self.persistent_canvas.width(),
            self.persistent_canvas.height(),
        );
        let mut new_c = Pixmap::new(nw, nh).expect("Failed to allocate Pixmap");
        new_c.fill(tiny_skia::Color::TRANSPARENT);
        let (cw, ch) = (ow.min(nw) as usize, oh.min(nh) as usize);
        let (src, dst) = (self.persistent_canvas.data(), new_c.data_mut());
        for y in 0..ch {
            let (si, di) = (y * ow as usize * 4, y * nw as usize * 4);
            dst[di..di + cw * 4].copy_from_slice(&src[si..si + cw * 4]);
        }
        self.persistent_canvas = new_c;
    }

    /// Accesses the backing CPU frame buffer.
    pub fn canvas(&self) -> &Pixmap {
        &self.persistent_canvas
    }

    /// Mutably accesses the backing CPU frame buffer.
    pub fn canvas_mut(&mut self) -> &mut Pixmap {
        &mut self.persistent_canvas
    }

    /// Updates the typography shaper and glyph database handle.
    pub fn set_text_context(&mut self, text_ctx: TextContext) {
        self.text_ctx = text_ctx;
    }

    /// Evicts an offscreen layer backing store by identifier.
    pub fn invalidate_layer(&mut self, id: LayerId) {
        self.layer_compositor.invalidate(id);
    }

    /// Rasterizes damaged rectangles of the scene into the persistent canvas.
    #[tracing::instrument(name = "render_damage", skip_all)]
    pub fn render_damage(
        &mut self,
        scene: &Scene,
        damage: &DamageRegion,
    ) -> Result<(), BackendError> {
        if damage.is_empty() {
            return Ok(());
        }
        let _span = tracing::info_span!("Stage6::TinySkiaRaster").entered();
        self.layer_compositor.advance_frame();
        let (cw, ch) = (
            self.persistent_canvas.width(),
            self.persistent_canvas.height(),
        );

        for rect in damage.rects() {
            let (x, y) = (
                (rect.x.floor() as i32).max(0) as u32,
                (rect.y.floor() as i32).max(0) as u32,
            );
            let (r, b) = (
                (rect.right().ceil() as u32).min(cw),
                (rect.bottom().ceil() as u32).min(ch),
            );
            if r <= x || b <= y {
                continue;
            }
            let (rw, rh) = (r - x, b - y);

            let mut scratch = match self.scratch_pixmap.take() {
                Some(mut p) if p.width() >= rw && p.height() >= rh => {
                    p.fill(tiny_skia::Color::TRANSPARENT);
                    p
                }
                _ => Pixmap::new(
                    rw.next_power_of_two().max(64),
                    rh.next_power_of_two().max(64),
                )
                .unwrap(),
            };

            self.tx_stack
                .push(SkiaTransform::from_translate(-(x as f32), -(y as f32)));
            let mut ctx = CommandContext {
                clip_stack: &mut self.clip_stack,
                text_ctx: &self.text_ctx,
                svg_cache: &mut self.svg_cache,
                image_cache: &mut self.image_cache,
                shadow_rasterizer: &mut self.shadow_rasterizer,
                layer_compositor: &mut self.layer_compositor,
                active_layers: &mut self.active_layers,
                tx_stack: &mut self.tx_stack,
                opacity_stack: &mut self.opacity_stack,
            };

            for chunk in scene.chunks.iter().filter(|c| c.bounds.intersects(rect)) {
                execute_commands(
                    &chunk.commands,
                    &mut scratch.as_mut(),
                    &mut ctx,
                    rw as f32,
                    rh as f32,
                );
            }
            self.tx_stack.pop();

            let (src, dst) = (scratch.data(), self.persistent_canvas.data_mut());
            for row in 0..rh as usize {
                let si = row * (scratch.width() * 4) as usize;
                let di = ((y as usize + row) * cw as usize + x as usize) * 4;
                dst[di..di + rw as usize * 4].copy_from_slice(&src[si..si + rw as usize * 4]);
            }
            self.scratch_pixmap = Some(scratch);
        }
        Ok(())
    }
}

impl Default for TinySkiaRenderer {
    fn default() -> Self {
        Self::new()
    }
}
