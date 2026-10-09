// Single responsibility: Single-window 2x2 combine backend for the Aurora UI engine.

pub mod quad_backend;

pub use quad_backend::{create_backend, quad_layout, quad_window, QuadBackend, QUAD_STRIP};
