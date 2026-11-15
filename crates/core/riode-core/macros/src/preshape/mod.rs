// Responsibility: Build-time asset pre-computation, text shaping, and host-side rasterization entry points.

pub mod image;
pub mod svg;
pub mod text;
pub mod util;

pub use text::pre_shaped_impl;
pub use image::include_image_impl;
pub use svg::include_svg_impl;
