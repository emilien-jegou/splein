// Hot-reloadable motion_playback UI (dev only): thin shim over
// `examples/motion_playback/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/motion_playback/view.rs`, save, and the window rebuilds.
extern crate aurora_shared as aurora;

#[path = "../../common/motion_ui.rs"]
mod motion_ui;

/// Peak travel of the sliding card, shared with the page so both agree.
/// Mirrors `TRAVEL_PX` in `examples/motion_playback/main.rs`.
pub const TRAVEL_PX: f32 = 180.0;
/// Seconds for one end-to-end slide. Mirrors `SLIDE_SECS` in `main.rs`.
pub const SLIDE_SECS: f32 = 2.4;

#[path = "../../motion_playback/view.rs"]
mod controls;

aurora::define_hot_ui!(controls::Page);
