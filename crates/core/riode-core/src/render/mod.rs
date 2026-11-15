//! Backend error type and shared raster helpers.
//!
//! Aurora draws through a backend it does not own: `riode-engine-wgpu`
//! implements the rasterizer behind a factory fn handed to
//! [`AppConfig::backend_factory`](crate::app::AppConfig::backend_factory).
//!
//! This module holds what is genuinely engine-side and backend-agnostic —
//! [`BackendError`], plus the small raster kernels every backend would
//! [`stroke_geometry`], [`swizzle_rgba_to_bgra`].
//!
//! If you find yourself adding a rasterizer *here*, stop — it belongs in the
//! backend crate. If two backends need the same ten lines of pixel math, it
//! belongs here.

pub mod error;
pub mod swizzle;

pub use error::BackendError;
pub use swizzle::swizzle_rgba_to_bgra;
