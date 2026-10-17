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
//! - **Elsewhere**: `riode-tiny-skia` (CPU) and `riode-wgpu` (raw GPU)
//!   implement the actual rasterizers behind a factory fn you hand to
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
//! use riode::prelude::*;
//!
//! // CPU (default): needs no GPU, and it's what the test harness uses.
//! App::config().backend(RenderBackend::TinySkia)
//!     .backend_factory(riode_tiny_skia::create_backend)
//!     .run(Page);
//!
//! // GPU: same scene, different painter.
//! App::config().backend(RenderBackend::Wgpu)
//!     .backend_factory(riode_engine_wgpu::create_backend)
//!     .run(Page);
//! ```
//!
//! Headless code never names a backend at all: [`crate::runtime::Engine::headless`] plus
//! `riode_tiny_skia::HeadlessRaster` (or the `Harness`) is the whole story.

pub mod blur;
pub mod error;
pub mod stroke_geometry;
pub mod swizzle;

pub use error::BackendError;
pub use swizzle::swizzle_rgba_to_bgra;

/// Selects the active rasterization and presentation engine.
///
/// A pure selection key: rasterizers live in `riode-tiny-skia` and `riode-wgpu` and are
/// supplied to [`AppConfig::backend_factory`](crate::app::AppConfig::backend_factory).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RenderBackend {
    #[default]
    TinySkia,
    /// Raw wgpu backend with analytical shaders (`riode-wgpu` crate).
    ///
    /// Traditional quad-based renderer with custom WGSL shaders.
    /// Shadows evaluate blur analytically in the fragment shader
    /// (zero CPU pixel work, zero cache thrash).
    Wgpu,
}
