// Declares the native dock rendering components, text engine, and motion trackers.

pub mod animation;
pub mod backdrop;
pub mod painter;
pub mod spectrum;
pub mod subelements;
pub mod text;

pub use painter::render_dock;
pub use text::TextEngine;
