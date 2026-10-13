// Single responsibility: 9-patch soft shadow kernel generation and clipped raster blitting.

use aurora::foundation::{BlendMode, Color, ResolvedRect, Shadow, ShadowLayer};
use aurora::render::blur::box_blur_alpha_with_falloff;
use tiny_skia::{Mask, PixmapMut, Transform};

/// Slice geometry for one layer: `(kernel_pad, corner_size, patch_dim)`.
///
/// Single source of truth shared by the renderer and the harness. The
/// corner slice extends past the inward blur support (`+2k`) so every
/// strip/center seam reads plateau on both sides; without the extension
/// the seam juxtaposes a still-ramping profile against the flat center
/// and draws as a dark square outline.
pub fn layer_geometry(radius: f32, shadow: &Shadow, layer: &ShadowLayer) -> (usize, usize, usize) {
    let pad = shadow.layer_pad(layer).ceil() as usize;
    // Same kernel the patch generator rounds to: `blur.round() as usize`
    // with `blur = softness.max(0.5)`, which is always >= 1.
    let kernel_int = layer.softness.max(0.5).round() as usize;
    let cs = radius.ceil().max(0.0) as usize + pad + 2 * kernel_int;
    (pad, cs, cs * 2)
}

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub struct ShadowPatchKey {
    pub radius_bits: u32,
    pub blur_bits: u32,
    pub extent_bits: u32,
    pub spread_bits: u32,
    pub falloff_tag: u8,
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
    falloff_lut: &[u8; 256],
    kernel_pad: usize,
    corner_size: usize,
    patch_dim: usize,
    spread: f32,
) -> ShadowPatch {
    let buf_len = patch_dim * patch_dim;
    super::fit_scratch_buffers([scratch_a, scratch_b, scratch_temp], buf_len, 0);
    scratch_a.fill(0);
    scratch_b.fill(0);

    // The occluder is drawn as infinite edge runs (solid quadrant) with the
    // corner arc cut out — never as a finite tip. A tip narrower than the
    // blur window dilutes to a ghost (peak ~ (2r/2blur)^2) while the solid
    // center fill keeps full opacity, which reads as a flat square slab.
    // Infinite runs hold the 255 plateau at any blur radius, matching Vello.
    //
    // Spread grows the occluder in place: edge runs move out by `spread`
    // while arc centers stay fixed and arc radii grow, exactly like growing
    // a rounded rect. `pad` already includes spread (see `layer_pad`), so
    // the inset backs it back out for shape placement.
    let p = kernel_pad as f32 - spread;
    let c = kernel_pad as f32 + radius.max(0.0);
    let reff = (radius + spread).max(0.0);

    for y in 0..patch_dim {
        for x in 0..patch_dim {
            let (fx, fy) = (x as f32, y as f32);
            let inside = fx >= p
                && fy >= p
                && (fx >= c || fy >= c || (fx - c) * (fx - c) + (fy - c) * (fy - c) <= reff * reff);
            if inside {
                scratch_a[y * patch_dim + x] = 255;
            }
        }
    }

    box_blur_alpha_with_falloff(
        scratch_a,
        scratch_b,
        scratch_temp,
        patch_dim,
        patch_dim,
        blur.round() as usize,
        falloff_lut,
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
    offset_x: f32,
    offset_y: f32,
    pad: f32,
    color: Color,
    opacity: f32,
    blend: BlendMode,
    transform: Transform,
    patch: &ShadowPatch,
    clip: Option<&Mask>,
) {
    let (canvas_w, canvas_h) = (pixmap.width() as i32, pixmap.height() as i32);
    let (tint_r, tint_g, tint_b) = (
        (color.r * 255.0).round() as u32,
        (color.g * 255.0).round() as u32,
        (color.b * 255.0).round() as u32,
    );
    let alpha_scale = ((opacity * 256.0).round() as u32).min(256);
    if alpha_scale == 0 {
        return;
    }

    let kernel_pad = pad.ceil();
    let origin_x = (rect.x + offset_x - kernel_pad + transform.tx).round() as i32;
    let origin_y = (rect.y + offset_y - kernel_pad + transform.ty).round() as i32;
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
        blend: BlendMode,
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
        match blend {
            BlendMode::Normal => {
                dst[idx] = ((r * a + dst[idx] as u32 * inv_a + 128) >> 8) as u8;
                dst[idx + 1] = ((g * a + dst[idx + 1] as u32 * inv_a + 128) >> 8) as u8;
                dst[idx + 2] = ((b * a + dst[idx + 2] as u32 * inv_a + 128) >> 8) as u8;
            }
            BlendMode::Multiply => {
                // Subtractive: tinted product replaces source-over lerp so the
                // shadow deepens the surface instead of washing it toward grey.
                let (d0, d1, d2) = (dst[idx] as u32, dst[idx + 1] as u32, dst[idx + 2] as u32);
                let (m0, m1, m2) = (
                    (r * d0 + 128) >> 8,
                    (g * d1 + 128) >> 8,
                    (b * d2 + 128) >> 8,
                );
                dst[idx] = ((m0 * a + d0 * inv_a + 128) >> 8) as u8;
                dst[idx + 1] = ((m1 * a + d1 * inv_a + 128) >> 8) as u8;
                dst[idx + 2] = ((m2 * a + d2 * inv_a + 128) >> 8) as u8;
            }
        }
        dst[idx + 3] = (a + ((dst[idx + 3] as u32 * inv_a + 128) >> 8)).min(255) as u8;
    }

    // Corner sampling mirrors within the [0..cs] notch block: the patch is
    // only diagonally symmetric (infinite runs are NOT center-symmetric),
    // so the far quadrants must be read mirrored, never forward. Forward
    // reads land in solid run-interior and paint square mass.
    for y in 0..cs {
        let row_top = y as usize * pd;
        let row_top_mir = (cs - 1 - y) as usize * pd;
        for x in 0..cs {
            let col_l = x as usize;
            let col_r = (cs - 1 - x) as usize;
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + x,
                origin_y + y,
                patch.mask[row_top + col_l],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                blend,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + total_w - cs + x,
                origin_y + y,
                patch.mask[row_top + col_r],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                blend,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + x,
                origin_y + total_h - cs + y,
                patch.mask[row_top_mir + col_l],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                blend,
                clip,
            );
            blend_pixel(
                dst_data,
                canvas_w,
                canvas_h,
                origin_x + total_w - cs + x,
                origin_y + total_h - cs + y,
                patch.mask[row_top_mir + col_r],
                alpha_scale,
                tint_r,
                tint_g,
                tint_b,
                blend,
                clip,
            );
        }
    }

    let edge_w = total_w - 2 * cs;
    if edge_w > 0 {
        for y in 0..cs {
            let (top_a, bot_a) = (
                patch.mask[y as usize * pd + cs as usize],
                patch.mask[(cs - 1 - y) as usize * pd + cs as usize],
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
                    blend,
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
                    blend,
                    clip,
                );
            }
        }
    }
    // Transpose of the top/bottom case: the side strips are `cs` pixels wide,
    // sampling the center-row profile (the straight-edge falloff), stretched
    // vertically. A single column here would leave the middle of both sides
    // hollow — and asymmetrically so, since the center-column sample hits
    // full coverage while the far-column sample hits empty blur.
    let edge_h = total_h - 2 * cs;
    if edge_h > 0 {
        for x in 0..cs {
            let (left_a, right_a) = (
                patch.mask[patch.corner_size as usize * pd + x as usize],
                patch.mask[patch.corner_size as usize * pd + (cs - 1 - x) as usize],
            );
            for y in 0..edge_h {
                blend_pixel(
                    dst_data,
                    canvas_w,
                    canvas_h,
                    origin_x + x,
                    origin_y + cs + y,
                    left_a,
                    alpha_scale,
                    tint_r,
                    tint_g,
                    tint_b,
                    blend,
                    clip,
                );
                blend_pixel(
                    dst_data,
                    canvas_w,
                    canvas_h,
                    origin_x + total_w - cs + x,
                    origin_y + cs + y,
                    right_a,
                    alpha_scale,
                    tint_r,
                    tint_g,
                    tint_b,
                    blend,
                    clip,
                );
            }
        }
    }

    // Solid umbra: the strips meet at the corners but never cover the middle,
    // which peeks out from under the card on offset shadows. Matches Vello.
    if edge_w > 0 && edge_h > 0 {
        for y in 0..edge_h {
            for x in 0..edge_w {
                blend_pixel(
                    dst_data,
                    canvas_w,
                    canvas_h,
                    origin_x + cs + x,
                    origin_y + cs + y,
                    255,
                    alpha_scale,
                    tint_r,
                    tint_g,
                    tint_b,
                    blend,
                    clip,
                );
            }
        }
    }
}
