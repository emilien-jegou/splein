// Single responsibility: Render module boundary, backend selector, and engine re-exports.

pub mod blur;
pub mod error;
pub mod stroke_geometry;
pub mod swizzle;

pub use error::BackendError;
pub use swizzle::swizzle_rgba_to_bgra;

/// Selects the active rasterization and presentation engine.
///
/// A pure selection key: rasterizers live in `aurora-tiny-skia` and `aurora-vello` and are
/// supplied to [`AppConfig::backend_factory`](crate::app::AppConfig::backend_factory).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RenderBackend {
    #[default]
    TinySkia,
    Vello,
}
