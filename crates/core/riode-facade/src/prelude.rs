// The application-facing prelude: everything engine-side comes from core's
// prelude; the element tag modules (`text`, `img`, `svg`) and their builder
// types are re-exported HERE — not in core — so the engine stays ignorant of
// which tags exist.
pub use riode_core::prelude::*;
pub use riode_elements::{img, svg, text};
pub use riode_elements::{image::ImageElement, text::TextElement};
pub use riode_elements_macros::{include_image, include_svg, pre_shaped};
