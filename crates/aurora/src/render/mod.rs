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
    /// Vello with CPU-staged coarse rasterization (`RendererOptions::use_cpu`).
    ///
    /// Still presents through wgpu (fine raster + blit stay on the GPU), but
    /// all stages up to fine rasterization run on the CPU — useful for
    /// comparing GPU-compute vs CPU-staged pipelines on the same scene.
    ///
    /// Reference/compare engine only: Vello 0.2 stages coarse work as
    /// single-threaded scalar dispatches over the full scene every animated
    /// frame (measured ~80x slower coarse than GPU on an RTX 2070), while
    /// [`RenderBackend::TinySkia`] is SIMD and damage-tiled. For an actually
    /// fast CPU rasterizer, use TinySkia.
    VelloCpu,
}
