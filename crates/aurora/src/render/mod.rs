// Single responsibility: Render module boundary, backend selector, and engine re-exports.

pub mod blur;
pub mod error;
pub mod swizzle;
pub mod tiny_skia;

#[cfg(feature = "vello")]
pub mod vello;

pub use error::BackendError;
pub use swizzle::swizzle_rgba_to_bgra;
pub use tiny_skia::TinySkiaRenderer;

#[cfg(feature = "vello")]
pub use vello::VelloRenderer;

/// Selects the active rasterization and presentation engine.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RenderBackend {
    #[default]
    TinySkia,
    #[cfg(feature = "vello")]
    Vello,
}
