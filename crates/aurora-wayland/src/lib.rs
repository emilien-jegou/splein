// Single responsibility: Wayland layer-shell SHM presentation backend for Aurora.
//
// [`WaylandBackend`] implements [`Backend`](aurora::app::runner::Backend) over a
// caller-owned `wl_surface` plus SHM pool; [`surface`] holds the overlay-surface setup
// helpers. The embedder (e.g. the splein daemon) keeps protocol setup, configure
// handling, and frame-callback pacing; this crate turns scenes into commits.

pub mod backend;
pub mod surface;

pub use backend::{damage_tiles, DamageTile, WaylandBackend};
pub use surface::{create_overlay_surface, set_passthrough};
