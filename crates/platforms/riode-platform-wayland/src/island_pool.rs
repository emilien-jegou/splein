// Single responsibility: compositor-island subsurface pool (Phase 2b).
//
// ⚠ SUPERSEDED as the island mechanism (2026-10-19): on Hyprland 0.56.2,
// regions set on layer-*subsurface* surfaces are silently dropped —
// core's `CBackgroundEffect` commit handler looks the view up via
// `CWLSurface::fromResource`, which is null for them, and returns early.
// Proven by the plugin's own rejection counters (`noview_layer` with zero
// matches). Live islands are top-level layer surfaces instead (see
// possible window-subsurface future; do not use it for layer islands.
// its full-scene buffer (robust fallback: content shows even if a
// subsurface is a frame late); each island duplicates its rect's pixels
// into its own SHM buffer so the compositor can treat it independently.
//
// Reactive for free: islands are re-extracted every frame, the pool diffs
// by paint order (create/destroy/resize), and synchronized-subsurface
// timing makes position/content/region updates atomic with the parent
// commit. Plain-transparent groups produce no islands and are untouched.
//
// Embedder contract: `State` needs `Dispatch` for `WlSurface`
// (SCTK `SurfaceData`), `WlSubsurface`, `WlRegion`,
// `ExtBackgroundEffectManagerV1`, `ExtBackgroundEffectSurfaceV1`, plus
// SCTK `delegate_shm!` (buffer releases). All user-data is `()` except
// the SCTK surface data; the capabilities event is ignored.

use smithay_client_toolkit::compositor::SurfaceData;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use wayland_client::protocol::wl_subcompositor::WlSubcompositor;
use wayland_client::protocol::{wl_compositor::WlCompositor, wl_region::WlRegion, wl_subsurface::WlSubsurface, wl_surface::WlSurface};
use wayland_client::{Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use riode_shared::render::swizzle_rgba_to_bgra;
use riode_shared::foundation::ResolvedRect;
use riode_shared::scene::Island;


struct PooledIsland {
    surface: WlSurface,
    subsurface: WlSubsurface,
    effect: Option<ExtBackgroundEffectSurfaceV1>,
    pool: SlotPool,
    w: u32,
    h: u32,
}

/// Clamp an island region to integer surface geometry: origin floored,
/// extent ceiled, minimum 1px.
fn island_geometry(region: &ResolvedRect) -> (i32, i32, u32, u32) {
    let x = region.x.floor() as i32;
    let y = region.y.floor() as i32;
    let w = (region.right().ceil() as i32 - x).max(1) as u32;
    let h = (region.bottom().ceil() as i32 - y).max(1) as u32;
    (x, y, w, h)
}

pub struct IslandPool {
    islands: Vec<PooledIsland>,
}

impl IslandPool {
    pub fn new() -> Self {
        Self {
            islands: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.islands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.islands.is_empty()
    }

    /// Sync the pool to this frame's islands.
    ///
    /// * `main_rgba` — the parent's premultiplied RGBA pixmap bytes
    ///   (`main_w` x `main_h`); island pixels are blitted from here.
    /// * `manager` — `None` keeps islands rendering without desktop
    ///   effects (protocol unsupported): surfaces still position content.
    #[allow(clippy::too_many_arguments)]
    pub fn sync<State>(
        &mut self,
        qh: &QueueHandle<State>,
        compositor: &WlCompositor,
        subcompositor: &WlSubcompositor,
        shm: &Shm,
        manager: Option<&ExtBackgroundEffectManagerV1>,
        parent: &WlSurface,
        islands: &[Island],
        main_rgba: &[u8],
        main_w: u32,
        main_h: u32,
    ) where
        State: Dispatch<WlSurface, SurfaceData>
            + Dispatch<WlSubsurface, ()>
            + Dispatch<WlRegion, ()>
            + Dispatch<ExtBackgroundEffectSurfaceV1, ()>
            + 'static,
    {
        // Drop surplus islands first: effect, then subsurface, then surface.
        while self.islands.len() > islands.len() {
            let old = self.islands.pop().unwrap();
            if let Some(effect) = old.effect {
                effect.destroy();
            }
            old.subsurface.destroy();
            old.surface.destroy();
        }
        // Grow: one synchronized subsurface per new island.
        while self.islands.len() < islands.len() {
            let surface = compositor.create_surface(qh, SurfaceData::default());
            let subsurface = subcompositor.get_subsurface(&surface, parent, qh, ());
            // Click-through: islands never swallow pointer events in the demo
            // phase (no interaction yet); the parent keeps its own region.
            let empty = compositor.create_region(qh, ());
            surface.set_input_region(Some(&empty));
            empty.destroy();
            let effect = manager.map(|m| effect_for_surface(m, qh, &surface));
            self.islands.push(PooledIsland {
                surface,
                subsurface,
                effect,
                pool: SlotPool::new(4, shm).expect("island SHM pool"),
                w: 0,
                h: 0,
            });
        }

        for (pooled, island) in self.islands.iter_mut().zip(islands.iter()) {
            let (x, y, w, h) = island_geometry(&island.region());
            pooled.subsurface.set_position(x, y);
            if pooled.w != w || pooled.h != h || pooled.w == 0 {
                pooled.pool = SlotPool::new((w * h * 4 * 3).max(4) as usize, shm)
                    .expect("island SHM pool resize");
                pooled.w = w;
                pooled.h = h;
            }
            let stride = (w * 4) as i32;
            if let Ok((buffer, canvas)) = pooled.pool.create_buffer(
                w as i32,
                h as i32,
                stride,
                wayland_client::protocol::wl_shm::Format::Argb8888,
            ) {
                blit_rgba_to_bgra(canvas, main_rgba, main_w, main_h, x, y, w, h);
                pooled.surface.attach(Some(buffer.wl_buffer()), 0, 0);
                pooled.surface.damage_buffer(0, 0, w as i32, h as i32);
            }
            // The region is island-local: the island surface IS the effect
            // area, so it always covers the whole subsurface.
            if let Some(effect) = &pooled.effect {
                    qh,
                    compositor,
                    effect,
                    &[ResolvedRect::new(0.0, 0.0, w as f32, h as f32)],
                );
            }
            // Synchronized: cached until the parent commit flushes it.
            pooled.surface.commit();
        }
    }
}

impl Default for IslandPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Copy an `(x, y, w, h)` tile from a premultiplied RGBA source into a
/// BGRA canvas, filling out-of-bounds source rows/pixels transparent.
/// Mirrors the swizzle in the SHM presentation backend.
///
/// `pub` for reuse by out-of-crate island managers (e.g. the
pub fn blit_rgba_to_bgra(
    canvas: &mut [u8],
    src: &[u8],
    src_w: u32,
    src_h: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) {
    let dst: &mut [u32] = bytemuck::cast_slice_mut(canvas);
    let src_px: &[u32] = bytemuck::cast_slice(src);
    let sw = src_w as usize;
    let sh = src_h as usize;
    for row in 0..h as usize {
        let sy = y as isize + row as isize;
        for col in 0..w as usize {
            let sx = x as isize + col as isize;
            let di = row * w as usize + col;
            if sx >= 0 && sy >= 0 && (sx as usize) < sw && (sy as usize) < sh {
                let si = sy as usize * sw + sx as usize;
                dst[di] = swizzle_pixel(src_px[si]);
            } else {
                dst[di] = 0;
            }
        }
    }
}

#[inline(always)]
fn swizzle_pixel(px: u32) -> u32 {
    let mut out = [0u32; 1];
    let src = [px];
    swizzle_rgba_to_bgra(&mut out, &src);
    out[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_floors_origin_and_ceils_extent() {
        let (x, y, w, h) = island_geometry(&ResolvedRect::new(10.5, 20.5, 30.0, 40.0));
        assert_eq!((x, y, w, h), (10, 20, 31, 41));
    }

    #[test]
    fn geometry_clamps_to_one_pixel() {
        let (x, y, w, h) = island_geometry(&ResolvedRect::new(5.0, 5.0, 0.2, 0.2));
        assert_eq!((x, y, w, h), (5, 5, 1, 1));
    }

    #[test]
    fn blit_copies_inside_and_clears_outside() {
        // 4x4 white source; blit a 2x2 tile at (-1,-1): only (0,0) lands.
        let src = vec![0xFFu8; 4 * 4 * 4];
        let mut canvas = vec![0xAAu8; 2 * 2 * 4];
        blit_rgba_to_bgra(&mut canvas, &src, 4, 4, -1, -1, 2, 2);
        let px: &[u32] = bytemuck::cast_slice(&canvas);
        assert_eq!(px[0], 0);
        assert_eq!(px[1], 0);
        assert_eq!(px[2], 0);
        // Source (0,0) is opaque white RGBA → BGRA white.
        assert_eq!(px[3], 0xFFFFFFFF);
    }
}
