// Hot-reloadable text_demo UI (dev only): thin shim over `examples/text_demo/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/text_demo/view.rs`, save, and the running window rebuilds.
extern crate aurora_shared as aurora;

#[path = "../../text_demo/view.rs"]
mod view;

aurora::define_hot_ui!(view::build_ui);
