// Single responsibility: Build-time asset baking macros for the element crate —
// the compile-time half of the `img`/`svg`/`text` elements.
//
// Everything here is host-side: files are read, rasterized and shaped during
// macro expansion, and the macros emit plain byte blobs that ride the runtime's
// retained `ImageSource` path. No runtime SVG parsing, no runtime resvg.
//
// `bake_src!` is the bridge the generic `view!` lowering uses: a string-literal
// `src=` on any lowercase tag expands to this macro (resolved through the
// `riode` facade), so the engine macro stays free of asset code and tag names.

#![feature(proc_macro_diagnostic)]

use proc_macro::TokenStream;

mod preshape;

/// Build-time text shaping: `pre_shaped!(font = "f.ttf", text = "...", size = 16.0)`.
#[proc_macro]
pub fn pre_shaped(tokens: TokenStream) -> TokenStream {
    match preshape::pre_shaped_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build-time SVG rasterization: `include_svg!("icon.svg", 64, 64)`.
#[proc_macro]
pub fn include_svg(tokens: TokenStream) -> TokenStream {
    match preshape::include_svg_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build-time image decode + resample: `include_image!("icon.png", 64, 64)`.
#[proc_macro]
pub fn include_image(tokens: TokenStream) -> TokenStream {
    match preshape::include_image_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Bakes a string-literal asset path at its natural size, dispatched by
/// extension (`.svg` rasterized, anything else decoded). The `view!` lowering
/// emits this for `src="assets/…"`; explicit sizing belongs to the element's
/// own `w`/`h`, never to the macro.
#[proc_macro]
pub fn bake_src(tokens: TokenStream) -> TokenStream {
    match preshape::bake_src_impl(tokens) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
