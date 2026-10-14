// Single responsibility: SHM layer-shell presentation of Aurora scenes on Wayland.
//
// Owns a retained CPU rasterizer plus the SHM pool behind one `wl_surface`. The embedder
// keeps protocol setup (registry, layer-shell configure, frame-callback pacing, input);
// this backend turns [`Present`] into attach + partial damage + commit. Unlike a simple
// fullscreen blit, only tiles inside the frame damage region are uploaded and advertised
// via `damage_buffer`.

use std::time::Instant;

use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use wayland_client::protocol::{wl_callback::WlCallback, wl_shm, wl_surface::WlSurface};
use wayland_client::{Dispatch, QueueHandle};

use riode::app::runner::backend::{Backend, Present, PresentReport};
use riode::foundation::{DamageRegion, ResolvedRect};
use riode::render::BackendError;
use riode::text::TextContext;
use riode_tiny_skia::TinySkiaRenderer;

/// Clamped integer damage tile: origin plus extent, in buffer pixels.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DamageTile {
    /// Left edge, floored and clamped to the buffer.
    pub x: u32,
    /// Top edge, floored and clamped to the buffer.
    pub y: u32,
    /// Width, ceiled and clamped to the buffer.
    pub w: u32,
    /// Height, ceiled and clamped to the buffer.
    pub h: u32,
}

/// Union of the frame damage region and extra damage, clamped to `w`x`h`.
///
/// Degenerate (empty or fully offscreen) rects are dropped, mirroring the tile math in
/// the CPU rasterizer so uploaded pixels and advertised damage agree exactly.
pub fn damage_tiles(
    damage: &DamageRegion,
    extra_damage: &[ResolvedRect],
    w: u32,
    h: u32,
) -> Vec<DamageTile> {
    let mut tiles = Vec::new();
    let mut push = |rect: &ResolvedRect| {
        let x = (rect.x.floor() as i32).max(0) as u32;
        let y = (rect.y.floor() as i32).max(0) as u32;
        let r = (rect.right().ceil() as i32).max(0).min(w as i32) as u32;
        let b = (rect.bottom().ceil() as i32).max(0).min(h as i32) as u32;
        if r > x && b > y {
            tiles.push(DamageTile {
                x,
                y,
                w: r - x,
                h: b - y,
            });
        }
    };
    for rect in damage.rects() {
        push(rect);
    }
    for rect in extra_damage {
        push(rect);
    }
    tiles
}

/// Wayland SHM presentation backend, generic over the embedder's dispatch state.
///
/// Constructed from an already-created `wl_surface` (see [`create_overlay_surface`](crate::surface::create_overlay_surface));
/// the embedder drives configure → [`Backend::resize`] and frame callbacks, and calls
/// [`Backend::present`] to rasterize and commit.
pub struct WaylandBackend<State> {
    qh: QueueHandle<State>,
    surface: WlSurface,
    pool: SlotPool,
    renderer: TinySkiaRenderer,
    width: u32,
    height: u32,
    /// Set on construction and resize: the next present repaints everything.
    needs_full: bool,
}

impl<State> WaylandBackend<State> {
    /// Attaches to a surface with a fresh SHM pool and rasterizer.
    pub fn new(
        qh: QueueHandle<State>,
        surface: WlSurface,
        shm: &Shm,
        width: u32,
        height: u32,
        text_ctx: TextContext,
    ) -> Result<Self, BackendError> {
        let (w, h) = (width.max(1), height.max(1));
        let pool = SlotPool::new((w * h * 4 * 3) as usize, shm)
            .map_err(|e| BackendError::init(format!("shm pool: {e:?}")))?;
        let mut renderer = TinySkiaRenderer::new();
        renderer.set_text_context(text_ctx);
        renderer.resize(w, h);
        Ok(Self {
            qh,
            surface,
            pool,
            renderer,
            width: w,
            height: h,
            needs_full: true,
        })
    }

    /// The surface this backend presents into.
    pub fn surface(&self) -> &WlSurface {
        &self.surface
    }
}

impl<State> Backend for WaylandBackend<State>
where
    State: Dispatch<WlCallback, WlSurface> + 'static,
{
    fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        if (w, h) == (self.width, self.height) {
            return;
        }
        self.width = w;
        self.height = h;
        // The pool grows on demand when new slots are allocated, so there is nothing
        // to reallocate here; the rasterizer buffer follows the surface size.
        self.renderer.resize(w, h);
        self.needs_full = true;
    }

    fn tag(&self) -> &'static str {
        "Wayland-SHM"
    }

    fn present(&mut self, frame: Present<'_>) -> Result<PresentReport, BackendError> {
        let _span = tracing::info_span!("Wayland::Present").entered();
        let (w, h) = (self.width, self.height);

        // Scroll shifts move retained pixels inside their clips (the blit in
        // `render_damage` below): the compositor must upload those clips as
        // well as the exposed strips, so expand them into the tile set.
        let mut scroll_aware = frame.damage.clone();
        for shift in frame.damage.scroll_shifts() {
            scroll_aware.push(shift.clip);
        }
        let tiles = if self.needs_full {
            vec![DamageTile { x: 0, y: 0, w, h }]
        } else {
            damage_tiles(&scroll_aware, frame.extra_damage, w, h)
        };
        if tiles.is_empty() {
            // Nothing changed: skip the commit entirely. SHM has no buffer age, so
            // unlike swapchain presentation there is no accumulated damage to replay.
            return Ok(PresentReport {
                raster: std::time::Duration::ZERO,
                present: std::time::Duration::ZERO,
            });
        }

        let t_raster = Instant::now();
        let mut paint = scroll_aware;
        if self.needs_full {
            paint.push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
        } else {
            for rect in frame.extra_damage {
                paint.push(*rect);
            }
        }
        self.renderer.render_damage(frame.scene, &paint)?;
        let raster = t_raster.elapsed();

        let t_present = Instant::now();
        let stride = (w * 4) as i32;
        let (buffer, canvas) = self
            .pool
            .create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888)
            .map_err(|e| BackendError::surface(format!("shm buffer: {e:?}")))?;

        // Pixmap is RGBA; Argb8888 on little-endian wants BGRA byte order.
        let src: &[u8] = self.renderer.canvas().data();
        let src: &[u32] = bytemuck::cast_slice(src);
        let dst: &mut [u32] = bytemuck::cast_slice_mut(canvas);
        for tile in &tiles {
            for row in 0..tile.h as usize {
                let y = tile.y as usize + row;
                let (si, di) = (
                    y * w as usize + tile.x as usize,
                    y * w as usize + tile.x as usize,
                );
                riode::render::swizzle_rgba_to_bgra(
                    &mut dst[di..di + tile.w as usize],
                    &src[si..si + tile.w as usize],
                );
            }
        }

        self.surface.frame(&self.qh, self.surface.clone());
        self.surface.attach(Some(buffer.wl_buffer()), 0, 0);
        for tile in &tiles {
            self.surface
                .damage_buffer(tile.x as i32, tile.y as i32, tile.w as i32, tile.h as i32);
        }
        self.surface.commit();
        self.needs_full = false;

        Ok(PresentReport {
            raster,
            present: t_present.elapsed(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(rects: &[(f32, f32, f32, f32)]) -> DamageRegion {
        let mut r = DamageRegion::new();
        for (x, y, w, h) in rects {
            r.push(ResolvedRect::new(*x, *y, *w, *h));
        }
        r
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn tiles_clamp_fractional_damage_to_buffer_pixels() {
        let tiles = damage_tiles(&region(&[(10.5, 20.5, 30.0, 40.0)]), &[], 800, 600);
        assert_eq!(
            tiles,
            vec![DamageTile {
                x: 10,
                y: 20,
                w: 31,
                h: 41,
            }]
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn tiles_drop_degenerate_and_offscreen_rects() {
        let tiles = damage_tiles(
            &region(&[
                (0.0, 0.0, 0.0, 8.0),
                (900.0, 900.0, 8.0, 8.0),
                (4.0, 4.0, 8.0, 8.0),
            ]),
            &[],
            800,
            600,
        );
        assert_eq!(
            tiles,
            vec![DamageTile {
                x: 4,
                y: 4,
                w: 8,
                h: 8
            }]
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn tiles_union_extra_damage() {
        let tiles = damage_tiles(
            &region(&[(0.0, 0.0, 8.0, 8.0)]),
            &[ResolvedRect::new(100.0, 100.0, 8.0, 8.0)],
            800,
            600,
        );
        assert_eq!(tiles.len(), 2);
    }
}
