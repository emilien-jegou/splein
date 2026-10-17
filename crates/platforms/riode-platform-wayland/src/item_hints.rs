// Single responsibility: `riode_item_v1` client plumbing for islands.
//
// Binds the plugin's hint manager (v2: raw material, no preset names),
// mints one item object per island surface, and uploads the per-commit
// material + shape hints (the *what* + the shape). Double-buffered:
// requests apply on the island surface's next commit, which for a
// top-level layer surface is its own commit.
//
// Client code is generated at compile time from
// `protocols/riode-item-v1.xml` (a copy of the plugin's protocol; keep
// the two in sync) via the `wayland-scanner` proc-macro — no codegen
// binary needed.
//
// Embedder contract: the event-loop `State` must implement
// `Dispatch<RiodeItemManagerV1, ()>` — a MANUAL empty impl, because the
// manager sends a `finished` event and `delegate_noop!` traps any received
// event — and `Dispatch<RiodeItemV1, ()>` (no events; `delegate_noop!` is
// fine).

pub mod generated {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    pub mod client {
        //! Client-side API of the riode_item_v1 protocol.
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("protocols/riode-item-v1.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("protocols/riode-item-v1.xml");
    }
}

pub use generated::client::riode_item_manager_v1::RiodeItemManagerV1;
pub use generated::client::riode_item_v1::RiodeItemV1;

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Dispatch, QueueHandle};

/// Bind the item-hint manager (version 2: raw material, no preset
/// names). Fails when the riode-glass plugin isn't loaded, or speaks
/// only v1 — the caller keeps running without hints, same as a missing
/// manager. There is deliberately no v1 fallback: the preset-name
/// channel is dead.
pub fn bind_item_manager<State>(
    globals: &GlobalList,
    qh: &QueueHandle<State>,
) -> Result<RiodeItemManagerV1, BindError>
where
    State: Dispatch<RiodeItemManagerV1, ()> + 'static,
{
    globals.bind(qh, 2..=2, ())
}

/// Mint one item object for an island surface. At most one active item per
/// surface (the protocol raises `item_exists` otherwise).
pub fn item_for_surface<State>(
    manager: &RiodeItemManagerV1,
    qh: &QueueHandle<State>,
    surface: &WlSurface,
) -> RiodeItemV1
where
    State: Dispatch<RiodeItemV1, ()> + 'static,
{
    manager.get_item(surface, qh, ())
}

/// Upload one island's packed glass frame (v4 wire: header + the 736
/// uniform floats, little-endian u32 words). Idempotent: sending the
/// same bytes every commit is harmless (pending state is simply
/// overwritten). An empty slice clears the hint (plain-blur fallback
/// applies).
pub fn upload_glass_frame(item: &RiodeItemV1, words: &[u32]) {
    if words.is_empty() {
        item.unset_glass();
        return;
    }
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    item.set_glass(bytes);
}

/// Upload one island's shape hint. Idempotent: sending the same values
/// every commit is harmless (pending state is simply overwritten).
///
/// `rect` and `radii` are island-surface-local logical px — for our
/// islands the surface IS the rect, so callers pass `(0, 0, w, h)`.
pub fn upload_item_shape(
    item: &RiodeItemV1,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    radii: [f64; 4],
) {
    if w <= 0.0 || h <= 0.0 {
        item.unset_shape();
        return;
    }
    let radii = radii.map(|r| r.max(0.0));
    item.set_shape(x, y, w, h, radii[0], radii[1], radii[2], radii[3]);
}
