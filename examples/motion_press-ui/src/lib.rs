// Hot-reloadable motion_press UI (dev only): thin shim over `examples/motion_press/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/motion_press/view.rs`, save, and the running window rebuilds.
extern crate aurora_shared as aurora;

#[path = "../../common/motion_ui.rs"]
mod motion_ui;

#[path = "../../motion_press/view.rs"]
mod view;

aurora::define_hot_ui!(view::Page);
