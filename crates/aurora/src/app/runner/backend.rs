// Single responsibility: Backend presentation contract and shared per-frame parameters.

use std::time::Duration;

use crate::foundation::{Color, DamageRegion, ResolvedRect};
use crate::render::BackendError;
use crate::scene::Scene;

/// Resolves the accumulated damage a swapchain buffer age requires.
pub trait DamageResolver {
    /// Returns every rectangle that must be repainted for the given buffer age.
    fn for_age(&mut self, age: u8) -> DamageRegion;
}

/// Scanline pass applied to a CPU framebuffer between rasterization and flip.
pub trait FramePostProcess {
    /// Post-processes the presentation buffer for the damaged rows in place.
    fn apply(&mut self, buffer: &mut [u32], damage: &DamageRegion, stride: usize, height: usize);
}

/// Target surface geometry and clear color for one presented frame.
pub struct SurfaceRequest {
    /// Base clear color painted behind the scene.
    pub background: Color,
    /// Presentation width in physical pixels.
    pub width: u32,
    /// Presentation height in physical pixels.
    pub height: u32,
}

/// Per-frame inputs shared by every presentation backend.
pub struct Present<'a> {
    /// Compiled display list for the frame, including extension overlays.
    pub scene: &'a Scene,
    /// Region changed since the previous frame; a raster culling hint.
    pub damage: &'a DamageRegion,
    /// Extra bounds extensions asked to have repainted this frame.
    pub extra_damage: &'a [ResolvedRect],
    /// Surface geometry and clear color to present into.
    pub surface: SurfaceRequest,
    /// Engine-owned accumulated damage lookup by swapchain buffer age.
    pub damage_resolver: &'a mut dyn DamageResolver,
    /// Scanline pass, only supplied to backends that advertise `supports_scanline`.
    pub post_process: Option<&'a mut dyn FramePostProcess>,
}

/// Records what a backend did while presenting one frame.
pub struct PresentReport {
    /// Time spent rasterizing the scene.
    pub raster: Duration,
    /// Time spent converting pixels and flipping the OS surface.
    pub present: Duration,
}

/// Presents compiled frames to an OS window surface.
pub trait Backend {
    /// Resizes the surface and any retained rasterizer buffers.
    fn resize(&mut self, width: u32, height: u32);

    /// Human-readable backend tag, e.g. `"CPU"` or `"GPU"`.
    fn tag(&self) -> &'static str;

    /// Whether this backend can expose its framebuffer to a scanline post-process pass.
    fn supports_scanline(&self) -> bool {
        false
    }

    /// Rasterizes and presents exactly one frame.
    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError>;
}
