// Responsibility: Build-time asset pre-computation, text shaping, and host-side rasterization entry points.

pub mod image;
pub mod svg;
pub mod text;
pub mod util;

pub use image::{bake_image, include_image_impl};
pub use svg::{bake_svg, include_svg_impl};
pub use text::pre_shaped_impl;
