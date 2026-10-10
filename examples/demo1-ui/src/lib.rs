// Hot-reloadable demo1 UI (dev only): thin shim over `examples/demo1/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/demo1/view.rs`, save, and the running window rebuilds.
extern crate aurora_shared as aurora;

#[path = "../../demo1/view.rs"]
mod view;

aurora::define_hot_ui!(view::Page);
