//! Single responsibility: the `riode` facade — one crate name application
//! authors depend on, re-exporting the core runtime, the element builders and
//! the `view!` macros. The macros emit `::riode::…` paths, so everything they
//! reference must resolve here.
//!
//! The prelude lives in this crate (not core) because it names element tag
//! modules, which are element-side, not engine-side. Core stays ignorant of
//! which tags exist; see `riode-core` for the engine, `riode-macros` for the
//! macro machinery (which knows exactly one tag: `g`).

pub use riode_core::*;
pub use riode_elements::{image, img, svg, text};

// `bake_src!` is invoked by absolute path from the `view!` expansion of a
// string-literal `src=`, so it must live at the facade root, not the prelude.
pub use riode_elements_macros::bake_src;

pub mod prelude;
