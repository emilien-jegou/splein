// Hot-reloadable demo2 UI (dev only): thin shim over `examples/demo2/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/demo2/view.rs`, save, and the running window rebuilds.
extern crate aurora_shared as aurora;

use aurora::dsl::IntoElement;
use aurora::prelude::*;

#[path = "../../demo2/view.rs"]
mod view;

/// demo2 builds its tree from pre-parsed icons. The tree borrows them, so they
/// live for the process: parsed once per library load (dev only, kilobytes).
fn icons() -> &'static view::Icons {
    use std::sync::OnceLock;
    static ICONS: OnceLock<view::Icons> = OnceLock::new();
    ICONS.get_or_init(view::Icons::load)
}

/// demo2 root: icons pinned process-wide, tree rebuilt per reload.
#[component(name = "examples::Demo2Root")]
fn Demo2Root() -> impl IntoElement {
    view::build_ui(icons())
}

aurora::define_hot_ui!(Demo2Root);
