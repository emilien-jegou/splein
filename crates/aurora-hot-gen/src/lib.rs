// Generative hot-reload UI (dev only): serves every example from one crate.
// Diplomatic: `extern crate aurora_shared as aurora` keeps the single shared
// runtime (never link `aurora` statically here). The build script generates
// `OUT_DIR/_gen.rs` from `AURORA_HOT_EXAMPLE`: it mounts the example's
// `hot.rs` (wiring + a single `define_hot_ui! { ... }` entry block).
extern crate aurora_shared as aurora;

include!(concat!(env!("OUT_DIR"), "/_gen.rs"));
