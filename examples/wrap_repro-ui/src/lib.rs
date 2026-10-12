// Hot-reloadable wrap_repro UI (dev only): thin shim over `examples/wrap_repro/view.rs`.
extern crate aurora_shared as aurora;

#[path = "../../wrap_repro/view.rs"]
mod view;

aurora::define_hot_ui!(view::Page);
