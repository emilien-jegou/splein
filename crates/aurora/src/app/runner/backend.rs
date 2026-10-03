// Single responsibility: Backend presentation contract and shared per-frame parameters.

use crate::app::extension::AppExtension;
use crate::foundation::{Color, DamageRegion};
use crate::render::BackendError;
use crate::runtime::FrameDiagnostics;
use crate::scene::Scene;

/// Per-frame inputs shared by every presentation backend.
pub struct PresentFrame<'a> {
    /// Compiled display list for the frame, including extension overlays.
    pub scene: &'a Scene,
    /// Region changed since the previous frame; a raster optimization hint.
    pub damage: &'a DamageRegion,
    /// Resolves accumulated damage for a swapchain buffer age.
    pub damage_for_age: &'a mut dyn FnMut(u8) -> DamageRegion,
    /// Base clear color painted behind the scene.
    pub background: Color,
    /// Presentation width in physical pixels.
    pub width: u32,
    /// Presentation height in physical pixels.
    pub height: u32,
    /// Receives raster and present timing measurements.
    pub diagnostics: &'a mut FrameDiagnostics,
    /// Extensions polled for overlay damage and CPU post-processing.
    pub extensions: &'a mut [Box<dyn AppExtension>],
}

/// Presents compiled frames to an OS window surface.
pub trait Backend {
    /// Resizes the surface and any retained rasterizer buffers.
    fn resize(&mut self, width: u32, height: u32);

    /// Human-readable backend tag, e.g. `"CPU"` or `"GPU"`.
    fn tag(&self) -> &'static str;

    /// Rasterizes and presents exactly one frame.
    fn present(&mut self, frame: PresentFrame<'_>) -> Result<(), BackendError>;
}
