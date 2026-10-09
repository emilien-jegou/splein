// Single responsibility: Direct blurred rasterizer for small rects or skewed transforms.

use aurora::foundation::{BlendMode, Color, Radius, ResolvedRect};
use aurora::render::blur::box_blur_alpha_with_falloff;
use tiny_skia::{Mask, Pixmap, PixmapMut, PixmapPaint, Transform};

/// Renders one shadow layer without 9-patch subdivision.
#[allow(clippy::too_many_arguments)]
pub fn render_direct_blur(
    scratch_a: &mut Vec<u8>,
    scratch_b: &mut Vec<u8>,
    scratch_temp: &mut Vec<u8>,
    pixmap: &mut PixmapMut,
    rect: &ResolvedRect,
    radius: f32,
    offset_x: f32,
    offset_y: f32,
    softness: f32,
    pad: f32,
    spread: f32,
    falloff_lut: &[u8; 256],
    color: Color,
    opacity: f32,
    blend: BlendMode,
    transform: Transform,
    clip: Option<&Mask>,
) {
    let scale = if softness >= 16.0 { 0.5f32 } else { 1.0f32 };
    let expansion = (pad + offset_x.abs().max(offset_y.abs())) * scale;
    let bounds = ResolvedRect::new(
        (rect.x * scale) - expansion + (offset_x * scale),
        (rect.y * scale) - expansion + (offset_y * scale),
        (rect.width * scale) + expansion * 2.0,
        (rect.height * scale) + expansion * 2.0,
    );

    let (bw, bh) = (
        (bounds.width.ceil() as u32).max(1),
        (bounds.height.ceil() as u32).max(1),
    );
    let buf_len = (bw * bh) as usize;
    scratch_a.resize(buf_len, 0);
    scratch_b.resize(buf_len, 0);
    scratch_temp.resize(buf_len, 0);
    scratch_a.fill(0);
    scratch_b.fill(0);

    let blur_r = ((softness * scale).round() as usize).max(1);
    // Spread inflates the occluder before blurring, matching the 9-patch
    // shape inflation (arc centers fixed, radii grown).
    let spread_s = spread * scale;
    let scaled_rect = ResolvedRect::new(
        -spread_s,
        -spread_s,
        rect.width * scale + 2.0 * spread_s,
        rect.height * scale + 2.0 * spread_s,
    );
    let shape_path = crate::path::build_rounded_path(
        &scaled_rect,
        Radius::Scalar((radius + spread).max(0.0) * scale),
    );
    let Some(mut temp_pixmap) = Pixmap::new(bw, bh) else {
        return;
    };

    let draw_tx = Transform::from_translate(-bounds.x, -bounds.y)
        .post_translate(offset_x * scale, offset_y * scale);
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);

    temp_pixmap.fill_path(
        &shape_path,
        &paint,
        tiny_skia::FillRule::Winding,
        draw_tx,
        None,
    );
    for (dst, src) in scratch_a.iter_mut().zip(temp_pixmap.data().chunks_exact(4)) {
        *dst = src[3];
    }
    box_blur_alpha_with_falloff(
        scratch_a,
        scratch_b,
        scratch_temp,
        bw as usize,
        bh as usize,
        blur_r,
        falloff_lut,
    );

    // Coverage only: node opacity rides on the draw's PixmapPaint (Normal)
    // or scales the resolve factor (Multiply), exactly once either way.
    for (dst, &alpha) in temp_pixmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(scratch_b.iter())
    {
        let a = alpha as f32 / 255.0;
        match blend {
            BlendMode::Normal => {
                dst[0] = ((color.r * a) * 255.0) as u8;
                dst[1] = ((color.g * a) * 255.0) as u8;
                dst[2] = ((color.b * a) * 255.0) as u8;
                dst[3] = (a * 255.0) as u8;
            }
            BlendMode::Multiply => {
                // Stores the raw tint; coverage rides in alpha and the
                // multiply resolve below reads both.
                dst[0] = ((color.r * 255.0).round()) as u8;
                dst[1] = ((color.g * 255.0).round()) as u8;
                dst[2] = ((color.b * 255.0).round()) as u8;
                dst[3] = (a * 255.0) as u8;
            }
        }
    }

    let final_tx = transform
        .pre_translate(bounds.x / scale, bounds.y / scale)
        .pre_scale(1.0 / scale, 1.0 / scale);

    if blend == BlendMode::Multiply {
        // tiny-skia has no multiply pixmap blend, so resolve per-pixel:
        // walk the affected canvas bbox, inverse-sample the temp coverage,
        // and fold the tinted product into the destination.
        let Some(inv) = final_tx.invert() else {
            return;
        };
        let (tw, th) = (bw as f32, bh as f32);
        let mut corners = [(0.0f32, 0.0f32), (tw, 0.0), (0.0, th), (tw, th)];
        for (px, py) in corners.iter_mut() {
            let (nx, ny) = (
                final_tx.sx * *px + final_tx.kx * *py + final_tx.tx,
                final_tx.ky * *px + final_tx.sy * *py + final_tx.ty,
            );
            (*px, *py) = (nx, ny);
        }
        let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
        let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (px, py) in corners {
            x0 = x0.min(px);
            y0 = y0.min(py);
            x1 = x1.max(px);
            y1 = y1.max(py);
        }
        let (cw, ch) = (pixmap.width() as i32, pixmap.height() as i32);
        let (x0, y0) = (x0.floor().max(0.0) as i32, y0.floor().max(0.0) as i32);
        let (x1, y1) = (x1.ceil().min(cw as f32) as i32, y1.ceil().min(ch as f32) as i32);
        let data = temp_pixmap.data();
        let (tw_i, th_i) = (bw as i32, bh as i32);
        for dy in y0..y1 {
            for dx in x0..x1 {
                let sx = (inv.sx * dx as f32 + inv.kx * dy as f32 + inv.tx).round() as i32;
                let sy = (inv.ky * dx as f32 + inv.sy * dy as f32 + inv.ty).round() as i32;
                if sx < 0 || sy < 0 || sx >= tw_i || sy >= th_i {
                    continue;
                }
                let t = &data[((sy * tw_i + sx) * 4) as usize..][..4];
                let a = ((t[3] as f32 * opacity).round() as u32).min(255);
                if a == 0 {
                    continue;
                }
                if let Some(mask) = clip {
                    let (mw, mh) = (mask.width() as i32, mask.height() as i32);
                    if dx >= mw || dy >= mh {
                        continue;
                    }
                    if mask.data()[(dy as usize) * (mw as usize) + dx as usize] == 0 {
                        continue;
                    }
                }
                let dst_data = pixmap.data_mut();
                let idx = ((dy * cw + dx) * 4) as usize;
                let inv_a = 255 - a;
                let (d0, d1, d2) = (
                    dst_data[idx] as u32,
                    dst_data[idx + 1] as u32,
                    dst_data[idx + 2] as u32,
                );
                let (m0, m1, m2) = (
                    ((t[0] as u32) * d0 + 128) >> 8,
                    ((t[1] as u32) * d1 + 128) >> 8,
                    ((t[2] as u32) * d2 + 128) >> 8,
                );
                dst_data[idx] = ((m0 * a + d0 * inv_a + 128) >> 8) as u8;
                dst_data[idx + 1] = ((m1 * a + d1 * inv_a + 128) >> 8) as u8;
                dst_data[idx + 2] = ((m2 * a + d2 * inv_a + 128) >> 8) as u8;
                dst_data[idx + 3] =
                    (a + ((dst_data[idx + 3] as u32 * inv_a + 128) >> 8)).min(255) as u8;
            }
        }
        return;
    }
    pixmap.draw_pixmap(
        0,
        0,
        temp_pixmap.as_ref(),
        &PixmapPaint {
            opacity,
            ..Default::default()
        },
        final_tx,
        clip,
    );
}
