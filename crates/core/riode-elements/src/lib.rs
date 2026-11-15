// Responsibility: Built-in UI leaf elements (text, img, svg) for the Riode engine.

pub mod config;
pub mod image;
pub mod svg;
pub mod text;

// Direct re-export: Lowering <img /> to img::build() without a fake trampoline module
pub use crate::image as img;
