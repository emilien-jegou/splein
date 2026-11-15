// Single responsibility: Tag-namespace entry for the `<svg>` view! tag.
//
// SVGs ship as build-time rasterized pixels (`include_svg!`), so the tag rides
// the same retained image leaf as `<img>` — no separate element type, no
// runtime SVG parsing. Default fit is `Contain`, the right semantics for icons.

use crate::image::ImageElement;
use riode_core::foundation::ImageFit;

/// `<svg>` lowers to `svg::build()`; `src=` fills it through `.src(...)`.
pub fn build() -> ImageElement {
    ImageElement {
        props: riode_core::dsl::leaf::LeafProps::default(),
        source: None,
        fit: ImageFit::Contain,
    }
}
