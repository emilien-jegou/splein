// Hot entry for `liquid_glass_new` (dev only): wiring + the single
// `define_hot_ui!` entry block.
#![allow(dead_code)]

#[path = "./view.rs"]
mod view;

riode::define_hot_ui! {
    root: view::Page,
    title: view::HOT_TITLE,
    size: (view::HOT_WIDTH, view::HOT_HEIGHT),
    background: view::HOT_BG,
}
