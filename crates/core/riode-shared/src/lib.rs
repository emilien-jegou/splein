// Single shared riode runtime image for the hot-reload dev loop.
//
// This crate exists for exactly one reason: its dynamic image is the *only* riode code
// (engine, reactive graph, and raster backends) in processes that load reloadable UI.
// The host dylib and every UI cdylib link this image instead of embedding their own
// copies, so hook scopes, ambient runtime stacks, signal epochs, and backend state exist
// exactly once — and real `#[component]` bodies execute across a `dlopen` boundary
// against the host's live runtime.
//
// Hot-loop crates depend on this crate and nothing else riode-flavored, then write
// `extern crate riode_shared as riode;`, after which every `::riode::` path
// (including `#[component]` and `view!` expansions) resolves here. Backend factories live
// under `riode_shared::tiny_skia` (and siblings as they are added).
//
// Dev-only, Linux-first: same toolchain on both sides (Rust has no stable ABI), and the
// loader keeps superseded images mapped (see `riode::hot_reload::dylib`).

pub use riode::*;
pub use riode_tiny_skia as tiny_skia;
pub use riode_engine_skia as skia;
pub use riode_engine_vello as vello;
pub use riode_engine_wgpu as wgpu;

include!("host.rs");
