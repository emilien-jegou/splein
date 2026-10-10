// Hot-reloadable selectable_text UI (dev only): thin shim over `examples/selectable_text/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/selectable_text/view.rs`, save, and the running window rebuilds.
extern crate aurora_shared as aurora;

#[path = "../../selectable_text/view.rs"]
mod view;

aurora::define_hot_ui!(view::build_ui);
