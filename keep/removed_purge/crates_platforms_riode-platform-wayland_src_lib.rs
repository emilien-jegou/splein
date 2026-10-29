// Single responsibility: Wayland layer-shell SHM presentation backend for Aurora.
//
// [`WaylandBackend`] implements [`Backend`](riode::app::runner::Backend) over a
// caller-owned `wl_surface` plus SHM pool; [`surface`] holds the overlay-surface setup
// helpers. The embedder (e.g. the splein daemon) keeps protocol setup, configure
// handling, and frame-callback pacing; this crate turns scenes into commits.

pub mod background_effect;
pub mod backend;
pub mod island_layers;
pub mod island_pool;
pub mod item_hints;
pub mod runner;
pub mod surface;

pub use background_effect::{bind_manager, effect_for_surface, upload_blur_region};
pub use island_pool::{blit_rgba_to_bgra, IslandPool};
pub use item_hints::{
    bind_item_manager, item_for_surface, upload_effect_stages, upload_glass_frame, upload_item_shape,
    RiodeItemManagerV1, RiodeItemV1,
};
pub use runner::{overlay_run, GlassMode, OverlayConfig, SurfacePolicy};

pub use backend::{damage_tiles, DamageTile, WaylandBackend};
pub use surface::{create_overlay_surface, set_passthrough};
