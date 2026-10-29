// Hot entry for `demo2` (dev only): icons pinned process-wide, tree rebuilt
// per reload, plus the single `define_hot_ui!` entry block. Mounted by the
// generative `riode-hot-gen-ui` crate; `#[path]`s resolve relative to this
// file. Window facts live once as `HOT_*` consts in `./view.rs`.
use riode::dsl::IntoElement;
use riode::prelude::*;

#[path = "./view.rs"]
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

riode::define_hot_ui! {
    root: Demo2Root,
    title: view::HOT_TITLE,
    size: (view::HOT_WIDTH, view::HOT_HEIGHT),
    background: view::HOT_BG,
}
