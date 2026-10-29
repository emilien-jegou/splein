// Single responsibility: Alpha-silhouette drop shadows — the layer's
// finished content alpha is the occluder outline; blur it per shadow
// layer, tint, and composite onto the parent behind the content.

use riode::foundation::{BlendMode, LightEnvironment, Shadow};
use riode::render::blur::box_blur_alpha_with_falloff;
use smallvec::SmallVec;
use tiny_skia::{Mask, Pixmap, PixmapMut, Transform};

/// Casts `shadow` from `source`'s alpha onto `dest`.
///
/// `source` is the layer's finished offscreen (filter already applied,
/// in-layer clips already baked in); `dest_offset` shifts the source's
/// (0,0) into `dest` coordinates (layer origin minus parent origin);
/// `transform` is the composed device matrix of the shadow's node — only
/// its linear part is read, to rotate/scale the per-layer offset.
///
/// One pass per shadow layer, largest extent first (tighter over softer,
/// identical order to the box rasterizers): extract the silhouette plane
/// once, then per layer two box iterations with the layer's falloff LUT
/// (sigma ≈ 1.15·r, matching the box path's kernel), tint, composite.
/// `spread` is ignored — CSS `drop-shadow` has none — and `Multiply`
/// resolves per-pixel exactly like the direct box path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    scratch_a: &mut Vec<u8>,
    scratch_b: &mut Vec<u8>,
    scratch_temp: &mut Vec<u8>,
    scratch_pixmap: &mut Option<Pixmap>,
    source: &Pixmap,
    dest: &mut PixmapMut,
    dest_offset: (f32, f32),
    shadow: &Shadow,
    light: &LightEnvironment,
    opacity: f32,
    transform: Transform,
    clip: Option<&Mask>,
) {
    if shadow.layers().is_empty() {
        return;
    }
    if shadow.spread != 0.0 {
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!("alpha shadow: spread is ignored (CSS drop-shadow has none)");
        }
    }
    let total_opacity = opacity * light.intensity;
    if total_opacity <= 0.0 {
        return;
    }
    let (w, h) = (source.width(), source.height());
    if w == 0 || h == 0 {
        return;
    }
    let (w_us, h_us) = (w as usize, h as usize);
    let len = w_us * h_us;
    super::fit_scratch_buffers([scratch_a, scratch_b, scratch_temp], len, 0);
    // Silhouette plane: the layer's premultiplied alpha, extracted once
    // (every layer blurs the same source; the box kernel never mutates it).
    for (dst, src) in scratch_a.iter_mut().zip(source.data().chunks_exact(4)) {
        *dst = src[3];
    }
    let Some(mut temp_pixmap) = (match scratch_pixmap.take() {
        Some(p) if p.width() == w && p.height() == h => {
            let mut p = p;
            p.fill(tiny_skia::Color::TRANSPARENT);
            Some(p)
        }
        _ => Pixmap::new(w, h),
    }) else {
        return;
    };

    // Largest extent first so tighter layers paint over softer ones.
    let mut order: SmallVec<[usize; 4]> = (0..shadow.layers().len()).collect();
    order.sort_by(|&a, &b| {
        shadow.layers()[b]
            .extent
            .partial_cmp(&shadow.layers()[a].extent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for i in order {
        let layer = &shadow.layers()[i];
        let layer_opacity = total_opacity * shadow.color.a * layer.opacity;
        if layer_opacity <= 0.0 {
            continue;
        }
        // Same kernel radius the box path derives from softness (no
        // downscale: silhouette pixels are real content, never geometry).
        let blur_r = (layer.softness.round() as usize).max(1);
        box_blur_alpha_with_falloff(
            scratch_a,
            scratch_b,
            scratch_temp,
            w_us,
            h_us,
            blur_r,
            &layer.falloff.lut(),
        );
        // Tint: coverage rides alpha; rgb follows the blend convention the
        // box path uses (premultiplied color for Normal, raw tint for the
        // per-pixel Multiply resolve below).
        for (dst, &alpha) in temp_pixmap
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(scratch_b.iter().take(len))
        {
            let a = alpha as f32 / 255.0;
            match shadow.blend {
                BlendMode::Normal => {
                    dst[0] = ((shadow.color.r * a) * 255.0) as u8;
                    dst[1] = ((shadow.color.g * a) * 255.0) as u8;
                    dst[2] = ((shadow.color.b * a) * 255.0) as u8;
                    dst[3] = (a * 255.0) as u8;
                }
                // Shadow honours Normal/Multiply only; the rest of the
                // `BlendMode` set folds to the multiply family (the
                // scene wire folds the same way — `scene/command.rs`).
                _ => {
                    dst[0] = ((shadow.color.r * 255.0).round()) as u8;
                    dst[1] = ((shadow.color.g * 255.0).round()) as u8;
                    dst[2] = ((shadow.color.b * 255.0).round()) as u8;
                    dst[3] = (a * 255.0) as u8;
                }
            }
        }

        // Per-layer offset in composed device px: the silhouette rides the
        // content's transform, so the displacement does too.
        let (ox, oy) = shadow.layer_offset(light, layer);
        let dx = transform.sx * ox + transform.kx * oy;
        let dy = transform.ky * ox + transform.sy * oy;
        let final_tx =
            Transform::from_translate(dest_offset.0 + dx, dest_offset.1 + dy);

        if shadow.blend == BlendMode::Multiply {
            // tiny-skia has no multiply pixmap blend; resolve per-pixel by
            // inverse-sampling the tinted coverage (direct-path pattern).
            let Some(inv) = final_tx.invert() else {
                return;
            };
            let (tw, th) = (w as f32, h as f32);
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
            let (cw, ch) = (dest.width() as i32, dest.height() as i32);
            let (x0, y0) = (x0.floor().max(0.0) as i32, y0.floor().max(0.0) as i32);
            let (x1, y1) = (
                x1.ceil().min(cw as f32) as i32,
                y1.ceil().min(ch as f32) as i32,
            );
            let data = temp_pixmap.data();
            let (tw_i, th_i) = (w as i32, h as i32);
            for dy_px in y0..y1 {
                for dx_px in x0..x1 {
                    let sx = (inv.sx * dx_px as f32
                        + inv.kx * dy_px as f32
                        + inv.tx)
                        .round() as i32;
                    let sy = (inv.ky * dx_px as f32
                        + inv.sy * dy_px as f32
                        + inv.ty)
                        .round() as i32;
                    if sx < 0 || sy < 0 || sx >= tw_i || sy >= th_i {
                        continue;
                    }
                    let t = &data[((sy * tw_i + sx) * 4) as usize..][..4];
                    let a =
                        ((t[3] as f32 * layer_opacity).round() as u32).min(255);
                    if a == 0 {
                        continue;
                    }
                    if let Some(mask) = clip {
                        let (mw, mh) = (mask.width() as i32, mask.height() as i32);
                        if dx_px >= mw || dy_px >= mh {
                            continue;
                        }
                        if mask.data()[(dy_px as usize) * (mw as usize) + dx_px as usize]
                            == 0
                        {
                            continue;
                        }
                    }
                    let dst_data = dest.data_mut();
                    let idx = ((dy_px * cw + dx_px) * 4) as usize;
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
            continue;
        }
        dest.draw_pixmap(
            0,
            0,
            temp_pixmap.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: layer_opacity,
                ..Default::default()
            },
            final_tx,
            clip,
        );
    }
    *scratch_pixmap = Some(temp_pixmap);
}
