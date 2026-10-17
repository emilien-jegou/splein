// Hot entry for `desktop_glass` (dev only): wiring + the single
// `define_hot_ui! { ... }` entry block. Mounted by the generative
// `riode-hot-gen-ui` crate; `#[path]`s resolve relative to this file.
// Window facts live once as `HOT_*` consts in `./view.rs`, referenced by
// both the release preview (`main.rs`) and this block — never mirrored.
// The overlay host (`apps/desktop-glass`) mounts the same dylib and reads
// the root entry per rebuild; it sizes from layer-shell configure and
// always paints transparent, ignoring HOT_BG.

#[path = "./view.rs"]
mod view;

riode::define_hot_ui! {
    root: view::Page,
    title: view::HOT_TITLE,
    size: (view::HOT_WIDTH, view::HOT_HEIGHT),
    background: view::HOT_BG,
}
