// Single responsibility: `ext-background-effect-v1` client plumbing for islands.
//
// Binds the manager global, mints one effect object per island surface, and
// uploads blur regions. This is the *where* channel only: it gates which
// pixels the compositor may treat (and later, which pixels riode-glass may
// render into) — it carries zero pixels and cannot render glass itself.
//
// Embedder contract: the event-loop `State` must implement
// `Dispatch<WlRegion, ()>` and `Dispatch<ExtBackgroundEffectSurfaceV1, ()>`
// (neither interface has events — `delegate_noop!` is fine), plus a MANUAL
// empty `Dispatch<ExtBackgroundEffectManagerV1, ()>` impl, because the
// compositor sends `capabilities` on the manager object and
// `delegate_noop!` traps any received event with `unreachable!()`.
// ```ignore
// wayland_client::delegate_noop!(State: wayland_client::protocol::wl_region::WlRegion);
// wayland_client::delegate_noop!(State: ExtBackgroundEffectSurfaceV1);
// impl Dispatch<ExtBackgroundEffectManagerV1, ()> for State {
//     fn event(&mut self, _: &ExtBackgroundEffectManagerV1, _: Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
// }

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::{wl_compositor::WlCompositor, wl_region::WlRegion, wl_surface::WlSurface};
use wayland_client::{Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use riode_shared::foundation::ResolvedRect;

/// Bind the background-effect manager (version 1). Fails when the
/// compositor doesn't implement the protocol — the caller keeps running
/// without desktop effects.
pub fn bind_manager<State>(
    globals: &GlobalList,
    qh: &QueueHandle<State>,
) -> Result<ExtBackgroundEffectManagerV1, BindError>
where
    State: Dispatch<ExtBackgroundEffectManagerV1, ()> + 'static,
{
    globals.bind(qh, 1..=1, ())
}

/// Mint one effect object for an island surface. At most one per surface
/// (the protocol raises `background_effect_exists` otherwise).
pub fn effect_for_surface<State>(
    manager: &ExtBackgroundEffectManagerV1,
    qh: &QueueHandle<State>,
    surface: &WlSurface,
) -> ExtBackgroundEffectSurfaceV1
where
    State: Dispatch<ExtBackgroundEffectSurfaceV1, ()> + 'static,
{
    manager.get_background_effect(surface, qh, ())
}

/// Upload one island's blur region (island-local coordinates) and clear it
/// when empty. Double-buffered: applies on the next commit of the island
/// surface the effect belongs to. For a synchronized subsurface that is
/// the parent commit that flushes it.
pub fn upload_blur_region<State>(
    qh: &QueueHandle<State>,
    compositor: &WlCompositor,
    effect: &ExtBackgroundEffectSurfaceV1,
    rects: &[ResolvedRect],
) where
    State: Dispatch<WlRegion, ()> + 'static,
{
    let mut live: Vec<(i32, i32, i32, i32)> = Vec::with_capacity(rects.len());
    for r in rects {
        let x = r.x.floor() as i32;
        let y = r.y.floor() as i32;
        let w = (r.right().ceil() as i32 - x).max(0);
        let h = (r.bottom().ceil() as i32 - y).max(0);
        if w > 0 && h > 0 {
            live.push((x, y, w, h));
        }
    }
    if live.is_empty() {
        effect.set_blur_region(None);
        return;
    }
    let region = compositor.create_region(qh, ());
    for (x, y, w, h) in live {
        region.add(x, y, w, h);
    }
    effect.set_blur_region(Some(&region));
    region.destroy();
}
