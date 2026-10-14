// Generative hot-reload UI (dev only): serves every example from one crate.
// Diplomatic: `extern crate riode_shared as riode` keeps the single shared
// runtime (never link `riode` statically here). The build script generates
// `OUT_DIR/_gen.rs` from `AURORA_HOT_EXAMPLE`: it mounts the example's
// `hot.rs` (wiring + a single `define_hot_ui! { ... }` entry block).
extern crate riode_shared as riode;

include!(concat!(env!("OUT_DIR"), "/_gen.rs"));
