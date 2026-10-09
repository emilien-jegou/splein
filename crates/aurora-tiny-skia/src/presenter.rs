// Single responsibility: Converts and presents premultiplied RGBA pixel buffers to OS surface displays.

use bon::bon;
use softbuffer::Buffer;
use std::num::NonZeroU32;
use tiny_skia::Pixmap;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use aurora::foundation::{DamageRegion, ResolvedRect};
use aurora::render::swizzle_rgba_to_bgra;

/// Converts and presents premultiplied RGBA pixel buffers to OS surface displays.
pub struct SurfacePresenter;

#[bon]
impl SurfacePresenter {
    /// Fast damaged-only presentation converting only active damage tiles via SIMD.
    #[builder]
    pub fn present_damaged<'a, D, W, F>(
        pixmap: &Pixmap,
        mut buffer: Buffer<'a, D, W>,
        damage: &DamageRegion,
        extra_damage: &[ResolvedRect],
        active_w: u32,
        active_h: u32,
        post_process: F,
    ) where
        D: HasDisplayHandle,
        W: HasWindowHandle,
        F: FnOnce(&mut [u32], &DamageRegion, usize, usize),
    {
        let _span = tracing::info_span!("Present::Damaged").entered();
        if active_w == 0 || active_h == 0 {
            return;
        }

        // Escape hatch for compositors where partial damage misbehaves: skips
        // the damage upload entirely and presents the whole viewport.
        // Diagnostic use (`AURORA_FULL_PRESENT=1`): if a missing highlight
        // appears with this set, the partial path is at fault, not the scene.
        // Read once: presenting runs per frame and must not pay an env lookup.
        static FULL_PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *FULL_PRESENT.get_or_init(|| std::env::var_os("AURORA_FULL_PRESENT").is_some()) {
            Self::present_viewport()
                .pixmap(pixmap)
                .buffer(buffer)
                .active_w(active_w)
                .active_h(active_h)
                .post_process(post_process)
                .call();
            return;
        }

        let age = buffer.age();
        tracing::debug!(
            age,
            damage_rects = damage.rects().len(),
            extra = extra_damage.len(),
            active_w,
            active_h,
            "presenting damaged"
        );
        if age == 0 {
            Self::present_viewport()
                .pixmap(pixmap)
                .buffer(buffer)
                .active_w(active_w)
                .active_h(active_h)
                .post_process(post_process)
                .call();
            return;
        }

        if damage.is_empty() && extra_damage.is_empty() {
            return;
        }

        let (dst_stride, src_stride) = (active_w as usize, pixmap.width() as usize);
        let (safe_w, safe_h) = (
            dst_stride.min(src_stride),
            (active_h as usize).min(pixmap.height() as usize),
        );
        let dst_pixels = buffer.as_mut();
        let src_all: &[u32] = bytemuck::cast_slice(pixmap.data());

        let mut rects = Vec::with_capacity(damage.rects().len() + extra_damage.len());
        {
            let _swizzle_span = tracing::info_span!("SIMD::SwizzleDamage").entered();
            for rect in damage.rects().iter().chain(extra_damage) {
                if let Some(sb_rect) = swizzle_rect(
                    dst_pixels, src_all, rect, dst_stride, src_stride, safe_w, safe_h,
                ) {
                    rects.push(sb_rect);
                }
            }
        }

        {
            let _pp_span = tracing::info_span!("Extensions::PostProcess").entered();
            post_process(dst_pixels, damage, dst_stride, safe_h);
        }

        if !rects.is_empty() {
            let _present_span = tracing::info_span!("OS::PresentWithDamage").entered();
            let _ = buffer.present_with_damage(&rects);
        }

        #[cfg(feature = "tracy")]
        {
            tracy_client::frame_mark();
        }
    }

    /// Full viewport SIMD copy fallback used on initial frame or window resize.
    #[builder]
    pub fn present_viewport<'a, D, W, F>(
        pixmap: &Pixmap,
        mut buffer: Buffer<'a, D, W>,
        active_w: u32,
        active_h: u32,
        post_process: F,
    ) where
        D: HasDisplayHandle,
        W: HasWindowHandle,
        F: FnOnce(&mut [u32], &DamageRegion, usize, usize),
    {
        if active_w == 0 || active_h == 0 {
            return;
        }

        let (dst_stride, src_stride) = (active_w as usize, pixmap.width() as usize);
        let (safe_w, safe_h) = (
            dst_stride.min(src_stride),
            (active_h as usize).min(pixmap.height() as usize),
        );
        let dst_pixels = buffer.as_mut();
        let src_all: &[u32] = bytemuck::cast_slice(pixmap.data());

        // Same resize-race guard as `swizzle_rect`: never address past the
        // live buffer.
        let rows = safe_h.min(dst_pixels.len() / dst_stride.max(1));
        for y in 0..rows {
            let (src_start, dst_start) = (y * src_stride, y * dst_stride);
            swizzle_rgba_to_bgra(
                &mut dst_pixels[dst_start..dst_start + safe_w],
                &src_all[src_start..src_start + safe_w],
            );
        }

        let empty_damage = DamageRegion::new();
        post_process(dst_pixels, &empty_damage, dst_stride, rows);
        let _ = buffer.present();

        #[cfg(feature = "tracy")]
        {
            tracy_client::frame_mark();
        }
    }
}

fn swizzle_rect(
    dst: &mut [u32],
    src: &[u32],
    r: &ResolvedRect,
    dst_stride: usize,
    src_stride: usize,
    safe_w: usize,
    safe_h: usize,
) -> Option<softbuffer::Rect> {
    // Genuine bounds from the slices themselves: the stride/size metadata
    // can disagree with the live buffer across a resize race (surface
    // reconfigured but buffer not yet re-allocated, or vice versa). Clamp
    // to reality and report once instead of panicking mid-frame.
    let dst_h = dst.len() / dst_stride.max(1);
    let src_h = src.len() / src_stride.max(1);
    let x0 = (r.x.floor() as i32).max(0) as usize;
    let y0 = (r.y.floor() as i32).max(0) as usize;
    let x1 = (r.right().ceil() as usize).min(safe_w);
    let y1 = (r.bottom().ceil() as usize).min(safe_h).min(dst_h).min(src_h);

    if x1 <= x0 || y1 <= y0 {
        static CLAMPED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        CLAMPED.get_or_init(|| {
            tracing::warn!(
                rect = ?r,
                dst_len = dst.len(),
                src_len = src.len(),
                dst_stride,
                src_stride,
                safe_w,
                safe_h,
                dst_h,
                src_h,
                "damage rect outside live buffer bounds; frame partially skipped"
            );
        });
        return None;
    }

    let rw = x1 - x0;
    for y in y0..y1 {
        let si = y * src_stride + x0;
        let di = y * dst_stride + x0;
        swizzle_rgba_to_bgra(&mut dst[di..di + rw], &src[si..si + rw]);
    }

    let w = NonZeroU32::new(rw as u32)?;
    let h = NonZeroU32::new((y1 - y0) as u32)?;
    Some(softbuffer::Rect {
        x: x0 as u32,
        y: y0 as u32,
        width: w,
        height: h,
    })
}
