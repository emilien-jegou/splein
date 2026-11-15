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

pub mod prelude;
