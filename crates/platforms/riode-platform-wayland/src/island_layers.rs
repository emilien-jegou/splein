// Single responsibility: one layer surface per compositor island.
//
// Why not subsurfaces: on Hyprland 0.56.2, `ext-background-effect-v1`
// regions set on layer-*subsurface* surfaces are silently dropped —
// core's commit handler looks up the view via
// `CWLSurface::fromResource`, which is null for them, and returns early
// (`BackgroundEffect.cpp`, `CBackgroundEffect::setResource`). Top-level
// layer surfaces get full views, so regions land. Each island is
// therefore its own `Layer::Overlay` surface, positioned with
// TOP|LEFT anchor + margins, carrying the island's pixels and its blur
// region. Paint order = creation order.
//
// The parent fullscreen surface stays as a robust fallback underneath.

use crate::{
    bind_manager, effect_for_surface, upload_blur_region, item_for_surface,
    upload_glass_frame, upload_item_shape,
};
use crate::island_pool::blit_rgba_to_bgra;
use crate::item_hints::{RiodeItemManagerV1, RiodeItemV1};

use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use wayland_client::protocol::{wl_output, wl_region, wl_surface::WlSurface};
use wayland_client::{Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use riode_shared::foundation::ResolvedRect;
use riode_shared::scene::Island;

pub struct IslandLayer {
    layer: LayerSurface,
    pool: SlotPool,
    pool_cap: usize,
    effect: Option<ExtBackgroundEffectSurfaceV1>,
    item: Option<RiodeItemV1>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pub configured: bool,
}

impl IslandLayer {
    pub fn wl_surface(&self) -> &WlSurface {
        self.layer.wl_surface()
    }
}

pub struct IslandLayerPool {
    islands: Vec<IslandLayer>,
}

impl IslandLayerPool {
    pub fn new() -> Self {
        Self {
            islands: Vec::new(),
        }
    }

    pub fn find_mut(&mut self, layer: &LayerSurface) -> Option<&mut IslandLayer> {
        self.islands
            .iter_mut()
            .find(|i| i.wl_surface() == layer.wl_surface())
    }

    /// Sync the pool to this frame's islands. Surface geometry follows the
    /// TIGHT island rect (no overscan pad): the plugin pads its sample box
    /// internally, and the parent surface underneath keeps showing any
    /// refracted overspill the tight blit clips.
    #[allow(clippy::too_many_arguments)]
    pub fn sync<State>(
        &mut self,
        qh: &QueueHandle<State>,
        layer_shell: &LayerShell,
        compositor: &CompositorState,
        shm: &Shm,
        output: Option<&wl_output::WlOutput>,
        manager: Option<&ExtBackgroundEffectManagerV1>,
        item_manager: Option<&RiodeItemManagerV1>,
        output_scale: f32,
        namespace: &str,
        islands: &[Island],
        main_rgba: &[u8],
        main_w: u32,
        main_h: u32,
    ) where
        State: Dispatch<WlSurface, smithay_client_toolkit::compositor::SurfaceData>
            + Dispatch<wl_region::WlRegion, ()>
            + Dispatch<ExtBackgroundEffectSurfaceV1, ()>
            + Dispatch<RiodeItemV1, ()>
            + Dispatch<
                smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
                smithay_client_toolkit::shell::wlr_layer::LayerSurfaceData,
            > + 'static,
    {
        // Drop surplus islands: effect, then layer (teardown is ordered).
        while self.islands.len() > islands.len() {
            let old = self.islands.pop().unwrap();
            if let Some(effect) = old.effect {
                effect.destroy();
            }
            old.layer.wl_surface().destroy();
        }
        // Grow: one overlay layer per new island, anchored TOP|LEFT and
        // positioned with margins. Size BEFORE first commit: core raises
        // a protocol error for a zero-sized axis without a full anchor.
        let have = self.islands.len();
        for island in &islands[have..] {
            let r = &island.rect;
            let x = r.x.floor() as i32;
            let y = r.y.floor() as i32;
            let w = (r.right().ceil() as i32 - x).max(1) as u32;
            let h = (r.bottom().ceil() as i32 - y).max(1) as u32;
            let surface = compositor.create_surface(qh);
            let layer = layer_shell.create_layer_surface(
                qh,
                surface,
                Layer::Overlay,
                Some(namespace.to_string()),
                output,
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.set_size(w, h);
            layer.set_margin(y, 0, 0, x);
            let empty = compositor.wl_compositor().create_region(qh, ());
            layer.wl_surface().set_input_region(Some(&empty));
            empty.destroy();
            let effect = manager.map(|m| effect_for_surface(m, qh, layer.wl_surface()));
            let item = item_manager.map(|m| item_for_surface(m, qh, layer.wl_surface()));
            layer.commit();
            self.islands.push(IslandLayer {
                layer,
                pool: SlotPool::new(
                    (w * h * 4 * 3).max(4) as usize,
                    shm,
                )
                .expect("island SHM pool"),
                pool_cap: (w * h * 4 * 3).max(4) as usize,
                effect,
                item,
                x,
                y,
                w,
                h,
                configured: false,
            });
        }

        for (pooled, island) in self.islands.iter_mut().zip(islands.iter()) {
            let r = &island.rect;
            let x = r.x.floor() as i32;
            let y = r.y.floor() as i32;
            let w = (r.right().ceil() as i32 - x).max(1) as u32;
            let h = (r.bottom().ceil() as i32 - y).max(1) as u32;
            if (pooled.x, pooled.y, pooled.w, pooled.h) != (x, y, w, h) {
                pooled.x = x;
                pooled.y = y;
                pooled.w = w;
                pooled.h = h;
                pooled.layer.set_size(w, h);
                pooled.layer.set_margin(y, 0, 0, x);
                let need = (w * h * 4 * 3).max(4) as usize;
                if need > pooled.pool_cap {
                    pooled.pool = SlotPool::new(need, shm).expect("island SHM pool resize");
                    pooled.pool_cap = need;
                }
            }
            if !pooled.configured {
                continue;
            }
            let stride = (w * 4) as i32;
            if let Ok((buffer, canvas)) = pooled.pool.create_buffer(
                w as i32,
                h as i32,
                stride,
                wayland_client::protocol::wl_shm::Format::Argb8888,
            ) {
                blit_rgba_to_bgra(canvas, main_rgba, main_w, main_h, x, y, w, h);
                let wl = pooled.layer.wl_surface().clone();
                wl.attach(Some(buffer.wl_buffer()), 0, 0);
                wl.damage_buffer(0, 0, w as i32, h as i32);
            }
            // The island surface IS the effect area: region covers it all.
            if let Some(effect) = &pooled.effect {
                upload_blur_region(
                    qh,
                    compositor.wl_compositor(),
                    effect,
                    &[ResolvedRect::new(0.0, 0.0, w as f32, h as f32)],
                );
            }
            // Hints: packed glass frame + rounded-rect shape,
            // island-local. The island rect is main-surface coords but
            // this surface sits at the island origin, so the pack is
            // rebased to (0,0): rect + layer members both shift by
            // -(x,y). The plugin then places the box at
            // surfacePos + box in every mode (parent-surface packs are
            // already surface-local, so no shift needed there).
            if let Some(item) = &pooled.item {
                let radii = shape_radii(island, w as f32, h as f32);
                let words = island.glass.as_ref().map(|g| {
                    riode_shared::foundation::pack_glass_wire(
                        ResolvedRect::new(0.0, 0.0, w as f32, h as f32),
                        &g.shifted(-(x as f32), -(y as f32)),
                        output_scale,
                    )
                });
                upload_glass_frame(item, words.as_deref().unwrap_or(&[]));
                upload_item_shape(item, 0.0, 0.0, w as f64, h as f64, radii);
            }
            pooled.layer.wl_surface().commit();
        }
    }
}

impl Default for IslandLayerPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Corner radii for the item-hint shape, from the island's glass material
/// (emit-stamped with the node's declared corners): `Radius::Max` becomes
/// a stadium (half the short edge), matching the engine's SDF.
fn shape_radii(island: &Island, w: f32, h: f32) -> [f64; 4] {
    use riode_shared::foundation::Radius;
    match island.radius() {
        Radius::ZERO => [0.0; 4],
        Radius::Scalar(v) => [v as f64; 4],
        Radius::Max => [w.min(h) as f64 / 2.0; 4],
        Radius::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => [
            top_left as f64,
            top_right as f64,
            bottom_right as f64,
            bottom_left as f64,
        ],
    }
}

/// Rebind helper: the demo binds the manager once; islands reuse it.
pub fn bind_bg_manager<State>(
    globals: &wayland_client::globals::GlobalList,
    qh: &QueueHandle<State>,
) -> Option<ExtBackgroundEffectManagerV1>
where
    State: Dispatch<ExtBackgroundEffectManagerV1, ()> + 'static,
{
    match bind_manager(globals, qh) {
        Ok(m) => {
            tracing::info!("ext-background-effect-v1 available: islands get blur regions");
            Some(m)
        }
        Err(e) => {
            tracing::warn!("no ext-background-effect-v1 ({e:?}): islands render without effects");
            None
        }
    }
}
