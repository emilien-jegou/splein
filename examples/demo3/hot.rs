// Hot entry for `demo3` (dev only): mounts the whole demo3 directory plus
// the single `define_hot_ui!` entry block.
//
// The demo3 modules address each other as `super::*`, so they mount under one
// parent exactly like the release crate mounts them (see `./mod.rs`).
// Mounted by the generative `riode-hot-gen-ui` crate. Window facts live
// once as `HOT_*` consts in `./view.rs`.
#[path = "./mod.rs"]
mod demo3;

riode::define_hot_ui! {
    root: demo3::view::Page,
    title: demo3::view::HOT_TITLE,
    size: (demo3::view::HOT_WIDTH, demo3::view::HOT_HEIGHT),
    background: demo3::view::HOT_BG,
}
