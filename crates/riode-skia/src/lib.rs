// Single responsibility: Skia engine slot for the Aurora UI engine.
//
// v1 strategy (per engine-skia.md): reuse the proven CPU rasterizer
// (`riode-tiny-skia`, cosmic-text shaping + atlas blits) behind the
// distinct `"skia"` tag so the engine is selectable end-to-end today.
// The raster core swaps to `skia-safe` (raster surface, later GPU
// gr_context) without touching callers: same `BackendFactory` signature,
// same `Backend` contract, same softbuffer present path.

#[cfg(feature = "desktop")]
mod backend;

#[cfg(feature = "desktop")]
pub use backend::SkiaBackend;

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use winit::window::Window;
#[cfg(feature = "desktop")]
use riode::app::runner::backend::Backend;
#[cfg(feature = "desktop")]
use riode::render::BackendError;
#[cfg(feature = "desktop")]
use riode::text::TextContext;

/// Builds a boxed Skia presentation backend for `AppConfig`.
#[cfg(feature = "desktop")]
pub fn create_backend(
    window: Arc<Window>,
    width: u32,
    height: u32,
    text: TextContext,
) -> Result<Box<dyn Backend>, BackendError> {
    Ok(Box::new(SkiaBackend::new(window, width, height, text)?))
}
