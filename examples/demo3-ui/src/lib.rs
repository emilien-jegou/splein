// Hot-reloadable demo3 UI (dev only): thin shim over `examples/demo3/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit anything under `examples/demo3/`, save, and the running window rebuilds.
//
// The demo3 modules address each other as `super::*`, so they mount under one
// parent exactly like the release crate mounts them (see `examples/demo3/mod.rs`).
extern crate aurora_shared as aurora;

#[path = "../../demo3/mod.rs"]
mod demo3;

aurora::define_hot_ui!(demo3::view::build_ui);
