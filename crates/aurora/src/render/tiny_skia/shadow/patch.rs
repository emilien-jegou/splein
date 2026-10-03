// Single responsibility: 9-patch soft shadow kernel generation and clipped raster blitting.

use crate::foundation::{ResolvedRect, Shadow};
use crate::render::blur::box_blur_alpha;
use tiny_skia::{Mask, PixmapMut, Transform};

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub struct ShadowPatchKey {
    pub radius_bits: u32,
    pub blur_bits: u32,
    pub spread_bits: u32,
}

pub struct ShadowPatch {
    pub mask: Vec<u8>,
    pub corner_size: usize,
    pub patch_dim: usize,
}

pub fn generate_patch(
    scratch_a: &mut Vec<u8>,
    scratch_b: &mut Vec<u8>,
    scratch_temp: &mut Vec<u8>,
    radius: f32,
    blur: f32,
    kernel_pad: usize,
    corner_size: usize,
    patch_dim: usize,
) -> ShadowPatch {
    let buf_len = patch_dim * patch_dim;
    scratch_a.resize(buf_len, 0);
    scratch_b.resize(buf_len, 0);
    scratch_temp.resize(buf_len, 0);
    scratch_a.fill(0);
    scratch_b.fill(0);

    let r_int = radius.ceil() as usize;
    let inner_size = if r_int == 0 {
        corner_size.saturating_sub(kernel_pad).max(1) * 2
    } else {
        2 * r_int
    };

    for y in 0..inner_size {
        for x in 0..inner_size {
            let inside = if radius <= 0.0 {
                true
            } else {
                let dx = r_int.abs_diff(x);
                let dy = if y < r_int {
                    r_int - y
                } else if y >= inner_size.saturating_sub(r_int) {
                    y - inner_size.saturating_sub(r_int)
                } else {
                    0
                };
                (dx * dx + dy * dy) as f32 <= radius * radius
            };
            if inside {
                let idx = (kernel_pad + y) * patch_dim + (kernel_pad + x);
                if idx < scratch_a.len() {
                    scratch_a[idx] = 255;
                }
            }
        }
    }

    box_blur_alpha(
        scratch_a,
        scratch_b,
        scratch_temp,
        patch_dim,
        patch_dim,
        blur.round() as usize,
    );
    ShadowPatch {
        mask: scratch_b.clone(),
        corner_size,
        patch_dim,
    }
}

pub fn blit_9patch(
    pixmap: &mut PixmapMut,
    rect: &ResolvedRect,
    shadow: &Shadow,
    opacity: f32,
    transform: Transform,
    patch: &ShadowPatch,
    clip: Option<&Mask>,
) {
    let (canvas_w, canvas_h) = (pixmap.width() as i32, pixmap.height() as i32);
    let c = shadow.color;
    let (tint_r, tint_g, tint_b) = (
        (c.r * 255.0).round() as u32,
        (c.g * 255.0).round() as u32,
        (c.b * 255.0).round() as u32,
    );
    let alpha_scale = ((opacity * c.a * 256.0).round() as u32).min(256);
    if alpha_scale == 0 {
        return;
    }

    let kernel_pad = (shadow.spread + 3.0 * shadow.blur).ceil();
    let origin_x = (rect.x + shadow.offset_x - kernel_pad + transform.tx).round() as i32;
    let origin_y = (rect.y + shadow.offset_y - kernel_pad + transform.ty).round() as i32;
    let (cs, pd) = (patch.corner_size as i32, patch.patch_dim);
    let (total_w, total_h) = (
        (rect.width + 2.0 * kernel_pad).round() as i32,
        (rect.height + 2.0 * kernel_pad).round() as i32,
    );
    let dst_data = pixmap.data_mut();

    #[inline(always)]
    fn blend_pixel(
        dst: &mut [u8],
        cw: i32,
        ch: i32,
        x: i32,
        y: i32,
        alpha: u8,
        scale: u32,
        r: u32,
        g: u32,
        b: u32,
        clip: Option<&Mask>,
    ) {
        if x < 0 || y < 0 || x >= cw || y >= ch || alpha == 0 {
            return;
        }
        let alpha = match clip {
            Some(mask) => {
                let (mw, mh) = (mask.width() as i32, mask.height() as i32);
                if x >= mw || y >= mh {
                    return;
                }
                let coverage = mask.data()[(y as usize) * (mw as usize) + x as usize] as u32;
                ((alpha as u32 * coverage + 127) / 255) as u8
            }
            None => alpha,
        };
        if alpha == 0 {
            return;
        }
        let a = (alpha as u32 * scale) >> 8;
        if a == 0 {
            return;
        }
        let idx = ((y * cw + x) * 4) as usize;
        let inv_a = 255 - a;
        dst[idx] = ((r * a + dst[idx] as u32 * inv_a + 128) >> 8) as u8;
        dst[idx + 1] = ((g * a + dst[idx + 1] as u32 * inv_a + 128) >> 8) as u8;
        dst[idx + 2] = ((b * a + dst[idx + 2] as u32 * inv_a + 128) >> 8) as u8;
        dst[idx + 3] = (a + ((dst[idx + 3] as u32 * inv_a + 128) >> 8)).min(255) as u8;
    }

    for y in 0..cs {
        let (row_top, row_bot) = (y as usize * pd, (patch.corner_size + y as usize) * pd);
        for x in 0..cs {
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + x,
                origin_y + y,
                patch.mask[row_top + x as usize],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + total_w - cs + x,
                origin_y + y,
                patch.mask[row_top + patch.corner_size + x as usize],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + x,
                origin_y + total_h - cs + y,
                patch.mask[row_bot + x as usize],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + total_w - cs + x,
                origin_y + total_h - cs + y,
                patch.mask[row_bot + patch.corner_size + x as usize],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                clip,
            );
        }
    }

    let edge_w = total_w - 2 * cs;
    if edge_w > 0 {
        for y in 0..cs {
            let (top_a, bot_a) = (
                patch.mask[y as usize * pd + cs as usize],
                patch.mask[(patch.corner_size + y as usize) * pd + cs as usize],
            );
            for x in 0..edge_w {
                blend_pixel(
                    dst_data,
                    canvas_w,
                    canvas_h,
                    origin_x + cs + x,
                    origin_y + y,
                    top_a,
                    alpha_scale,
                    tint_r,
                    tint_g,
                    tint_b,
                    clip,
                );
                blend_pixel(
                    dst_data,
                    canvas_w,
                    canvas_h,
                    origin_x + cs + x,
                    origin_y + total_h - cs + y,
                    bot_a,
                    alpha_scale,
                    tint_r,
                    tint_g,
                    tint_b,
                    clip,
                );
            }
        }
    }
    let edge_h = total_h - 2 * cs;
    if edge_h > 0 {
        let (left_a, right_a) = (
            patch.mask[patch.corner_size * pd],
            patch.mask[patch.corner_size * pd + patch.corner_size],
        );
        for y in 0..edge_h {
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x,
                origin_y + cs + y,
                left_a,
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + total_w - cs,
                origin_y + cs + y,
                right_a,
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                clip,
            );
        }
    }
}
