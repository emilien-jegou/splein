// Hot entry for `shadow_lab` (dev only): wiring + the single `define_hot_ui!`
// entry block. Mounted by the generative `riode-hot-gen-ui` crate;
// `#[path]`s resolve relative to this file. Window facts live once as
// `HOT_*` consts in `./view.rs`.
#![allow(dead_code)]

#[path = "../common/motion_ui.rs"]
mod motion_ui;

#[path = "./view.rs"]
mod view;

riode::define_hot_ui! {
    root: view::Page,
    title: view::HOT_TITLE,
    size: (view::HOT_WIDTH, view::HOT_HEIGHT),
    background: view::HOT_BG,
}
