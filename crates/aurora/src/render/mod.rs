//! Backend selection and shared raster helpers.
//!
//! Aurora draws through backends it does not own. This module holds the two things that
//! *are* engine-side: the [`RenderBackend`] selector enum, and the small raster kernels
//! both backends share rather than reimplementing twice.
//!
//! # The split
//!
//! - **Here**: [`RenderBackend`] (which painter to use), [`BackendError`], plus shared
//!   kernels — [`blur`] (box-blur with shadow falloff), [`stroke_geometry`],
//!   [`swizzle_rgba_to_bgra`].
//! - **Elsewhere**: `aurora-tiny-skia` (CPU), `aurora-vello` (GPU compute) and
//!   `aurora-wgpu` (raw GPU) implement the actual
//!   rasterizers behind a factory fn you hand to
//!   [`AppConfig::backend_factory`](crate::app::AppConfig::backend_factory).
//!
//! If you find yourself adding a rasterizer *here*, stop — it belongs in a backend crate.
//! If both backends need the same ten lines of pixel math, it belongs here.
//!
//! # Showcase
//!
//! Backend selection is one enum at startup; everything downstream is backend-agnostic:
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! // CPU (default): needs no GPU, and it's what the test harness uses.
//! App::config().backend(RenderBackend::TinySkia)
//!     .backend_factory(aurora_tiny_skia::create_backend)
//!     .run(Page);
//!
//! // GPU: same scene, different painter.
//! App::config().backend(RenderBackend::Vello)
//!     .backend_factory(aurora_vello::create_backend)
//!     .run(Page);
//! ```
//!
//! Headless code never names a backend at all: [`crate::runtime::Engine::headless`] plus
//! `aurora_tiny_skia::HeadlessRaster` (or the `Harness`) is the whole story.

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
    /// Raw wgpu backend with analytical shaders (`aurora-wgpu` crate).
    ///
    /// Traditional quad-based renderer with custom WGSL shaders instead of
    /// Vello's compute rasterizer. Shadows evaluate blur analytically in
    /// the fragment shader (zero CPU pixel work, zero cache thrash).
    Wgpu,
}
