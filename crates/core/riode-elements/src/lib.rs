//! Single responsibility: the built-in leaf elements — `text`, `img`, `svg` —
//! shipped as plain components with no privileges the engine or macro can see.
//!
//! Every tag is a module exposing a zero-arg `build()` entry point; `view!`
//! lowers `<tag>` to `tag::build()` and the call-site import resolves it,
//! exactly like a third-party element. `img` and `svg` share the image leaf;
//! `svg` differs only in its default fit (`Contain`, the right semantics for
//! icons) because SVGs ship as build-time rasterized pixels (`include_svg!`).

pub mod config;
pub mod image;
pub mod img;
pub mod svg;
pub mod text;
