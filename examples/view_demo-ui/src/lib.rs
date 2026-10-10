// Hot-reloadable view_demo UI (dev only): thin shim over `examples/view_demo/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/view_demo/view.rs`, save, and the running window rebuilds.
extern crate aurora_shared as aurora;

#[path = "../../common/motion_ui.rs"]
mod motion_ui;

#[path = "../../view_demo/view.rs"]
mod view;

aurora::define_hot_ui!(view::PageEntry);
