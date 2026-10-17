// Hot UI cdylib for the dock overlay (dev only).
//
// The view sources live HERE, next to the app — not under `examples/`.
// The runner's dev loop watches `apps/dock/src`, rebuilds this cdylib,
// and swaps it live without restarting the overlay process. Window facts
// live once as `HOT_*` consts in `./view.rs`, read by the host per
// rebuild — never mirrored.

extern crate riode_shared as riode;

mod dock;
mod icons;
mod registry;
mod view;

riode::define_hot_ui! {
    root: view::Page,
    title: view::HOT_TITLE,
    size: (view::HOT_WIDTH, view::HOT_HEIGHT),
    background: view::HOT_BG,
}
