// Single responsibility: Headless CPU rasterization of the compiled scene for pixel verification.

use tiny_skia::Pixmap;

use riode::foundation::DamageRegion;
use riode::scene::Scene;
use riode::text::TextContext;

use crate::renderer::TinySkiaRenderer;

/// Scene inputs required to rasterize one headless frame.
pub struct HeadlessFrame<'a> {
    /// Compiled display list to rasterize.
    pub scene: &'a Scene,
    /// Damaged rectangles to replay into the retained canvas.
    pub damage: &'a DamageRegion,
    /// Logical viewport the retained canvas must cover.
    pub size: (u32, u32),
    /// Typography context supplying shaped runs and loaded fonts.
    pub text_ctx: &'a TextContext,
}

/// Persistent CPU rasterizer replaying damage into a retained canvas.
pub struct HeadlessRaster {
    renderer: Option<TinySkiaRenderer>,
}

impl HeadlessRaster {
    /// Creates an empty rasterizer that materializes on first draw.
    pub fn new() -> Self {
        Self { renderer: None }
    }

    /// Rasterizes one frame and returns the retained canvas.
    pub fn draw(&mut self, frame: HeadlessFrame<'_>) -> &Pixmap {
        let renderer = self.renderer.get_or_insert_with(TinySkiaRenderer::new);
        renderer.set_text_context(frame.text_ctx.clone());
        renderer.resize(frame.size.0, frame.size.1);
        renderer
            .render_damage(frame.scene, frame.damage)
            .expect("headless rasterization failed");
        renderer.canvas()
    }

    /// Rasterizes the engine's current scene damage into the retained canvas.
    pub fn draw_engine(&mut self, engine: &mut riode::runtime::Engine) -> &Pixmap {
        let size = engine.logical_size();
        let text_ctx = engine.text_context();
        let damage = engine.current_damage().clone();
        self.draw(HeadlessFrame {
            scene: engine.scene(),
            damage: &damage,
            size,
            text_ctx: &text_ctx,
        })
    }
}

impl Default for HeadlessRaster {
    fn default() -> Self {
        Self::new()
    }
}
