// Hot entry for `motion_playback` (dev only): wiring plus the single
// `define_hot_ui!` entry block. Mounted by the generative
// `aurora-hot-gen-ui` crate; `#[path]`s resolve relative to this file.
// Window facts live once as `HOT_*` consts in `./view.rs`; slide consts
// (`TRAVEL_PX`/`SLIDE_SECS`) live there too — nothing mirrors `main.rs`.
#[path = "../common/motion_ui.rs"]
mod motion_ui;

#[path = "./view.rs"]
mod controls;

aurora::define_hot_ui! {
    root: controls::Page,
    title: controls::HOT_TITLE,
    size: (controls::HOT_WIDTH, controls::HOT_HEIGHT),
    background: controls::HOT_BG,
}
