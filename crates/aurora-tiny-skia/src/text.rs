// Single responsibility: Glyph bitmap Porter-Duff alpha compositing via TinySkia.

use aurora::foundation::{Color, Point};
use aurora::text::{TextContext, TextLayout};
use tiny_skia::{
    FilterQuality, Mask, Pixmap, PixmapMut, PixmapPaint, Point as SkiaPoint, PremultipliedColorU8,
    Transform,
};

/// Parameters for typography run rasterization.
pub struct TextRenderParams<'a> {
    /// Translation offset in destination buffer.
    pub origin: Point,
    /// Shaped text layout data.
    pub layout: &'a TextLayout,
    /// Text foreground color.
    pub color: Color,
    /// Global opacity multiplier.
    pub opacity: f32,
    /// Active coordinate transformation.
    pub transform: Transform,
    /// Clipping mask, if active.
    pub clip: Option<&'a Mask>,
}

/// Renders shaped typography glyph runs directly into a destination pixel buffer.
pub fn render_text(pixmap: &mut PixmapMut, text_ctx: &TextContext, params: TextRenderParams) {
    if params.layout.lines.is_empty() {
        return;
    }

    // Skewed or scaled transforms change glyph shapes, not just their origins,
    // so those go through the backend's transformed blit. Pure translations
    // keep the pixel-snapped fast path below, bit-identical to before.
    if params.transform.has_skew() || params.transform.has_scale() {
        render_text_transformed(pixmap, text_ctx, params);
        return;
    }

    render_text_aligned(pixmap, text_ctx, params);
}

/// Renders glyph runs snapped to the pixel grid for translation-only transforms.
fn render_text_aligned(pixmap: &mut PixmapMut, text_ctx: &TextContext, params: TextRenderParams) {
    let pw = pixmap.width();
    let ph = pixmap.height();
    let pixels = pixmap.pixels_mut();

    let ink_r = (params.color.r * 255.0).round() as u8;
    let ink_g = (params.color.g * 255.0).round() as u8;
    let ink_b = (params.color.b * 255.0).round() as u8;
    let total_opacity = params.opacity * params.color.a;

    // Coverage-to-alpha table for this run: opacity is constant across the run,
    // so the per-pixel float multiply + round becomes one table lookup.
    let alpha_lut: [u8; 256] = std::array::from_fn(|i| (i as f32 * total_opacity).round() as u8);
    // Mask dimensions hoisted out of the per-pixel loop below.
    let (clip_w, clip_h) = match params.clip {
        Some(mask) => (mask.width(), mask.height()),
        None => (0, 0),
    };

    // Reused staging buffer for glyph coverage. `raster_glyph` invokes its
    // closure while holding the font-system locks, so the bitmap is copied out
    // and blitted after the locks are released instead of compositing inline.
    let mut coverage = Vec::with_capacity(256);
    // Scratch accumulation for fractional blits, resized per glyph below.
    let mut fractional = Vec::new();

    for line in &params.layout.lines {
        for g in &line.glyphs {
            let mut pt = SkiaPoint::from_xy(
                params.origin.x + line.align_offset + g.point.x,
                params.origin.y + g.point.y,
            );
            params.transform.map_point(&mut pt);
            // Floor, not round: the fractional remainder is carried into the blit
            // (see below) instead of being discarded.
            let (base_x, base_y) = (pt.x.floor() as i32, pt.y.floor() as i32);
            let (fx, fy) = (pt.x - pt.x.floor(), pt.y - pt.y.floor());

            let mut placed: Option<(i32, i32, u32, u32)> = None;
            text_ctx.raster_glyph(g.cache_key, |img_opt| {
                if let Some(img) = img_opt {
                    coverage.clear();
                    coverage.extend_from_slice(img.data);
                    placed = Some((base_x + img.left, base_y - img.top, img.width, img.height));
                }
            });

            if let Some((start_x, start_y, w, h)) = placed {
                // Exact pixel origins keep the historical fast path bit-identical;
                // anything fractional distributes coverage instead of rounding it
                // away (see `blit_glyph_fractional`).
                if fx == 0.0 && fy == 0.0 {
                    blit_glyph(
                        pixels,
                        pw,
                        ph,
                        &coverage,
                        w,
                        h,
                        start_x,
                        start_y,
                        ink_r,
                        ink_g,
                        ink_b,
                        &alpha_lut,
                        params.clip,
                        clip_w,
                        clip_h,
                    );
                } else {
                    blit_glyph_fractional(
                        pixels,
                        pw,
                        ph,
                        &coverage,
                        w,
                        h,
                        start_x,
                        start_y,
                        fx,
                        fy,
                        ink_r,
                        ink_g,
                        ink_b,
                        &alpha_lut,
                        params.clip,
                        clip_w,
                        clip_h,
                        &mut fractional,
                    );
                }
            }
        }
    }
}

/// Renders glyph runs through the backend so rotation and scale shape the
/// glyphs themselves. Each glyph becomes a tinted coverage pixmap drawn with
/// the full transform from its fractional origin: the engine carries origin +
/// transform, tiny-skia does the filtering and clipping.
fn render_text_transformed(
    pixmap: &mut PixmapMut,
    text_ctx: &TextContext,
    params: TextRenderParams,
) {
    let ink_r = (params.color.r * 255.0).round() as u8;
    let ink_g = (params.color.g * 255.0).round() as u8;
    let ink_b = (params.color.b * 255.0).round() as u8;
    let opacity = (params.opacity * params.color.a).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let paint = PixmapPaint {
        opacity,
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };

    // Staging buffer for glyph coverage, copied out from under the
    // font-system locks like the aligned path does.
    let mut coverage = Vec::with_capacity(256);

    for line in &params.layout.lines {
        for g in &line.glyphs {
            // Fractional origin: no pixel-grid snap on this path.
            let base_x = params.origin.x + line.align_offset + g.point.x;
            let base_y = params.origin.y + g.point.y;

            let mut placed: Option<(f32, f32, u32, u32)> = None;
            text_ctx.raster_glyph(g.cache_key, |img_opt| {
                if let Some(img) = img_opt {
                    coverage.clear();
                    coverage.extend_from_slice(img.data);
                    placed = Some((
                        base_x + img.left as f32,
                        base_y - img.top as f32,
                        img.width,
                        img.height,
                    ));
                }
            });

            let Some((gx, gy, w, h)) = placed else {
                continue;
            };
            if w == 0 || h == 0 || coverage.len() < (w * h) as usize {
                continue;
            }
            let Some(mut glyph) = Pixmap::new(w, h) else {
                continue;
            };
            for (dst, &cov) in glyph.pixels_mut().iter_mut().zip(coverage.iter()) {
                if cov == 0 {
                    continue;
                }
                let tint = |c: u8| ((c as u32 * cov as u32 + 127) / 255) as u8;
                if let Some(px) =
                    PremultipliedColorU8::from_rgba(tint(ink_r), tint(ink_g), tint(ink_b), cov)
                {
                    *dst = px;
                }
            }
            pixmap.draw_pixmap(
                0,
                0,
                glyph.as_ref(),
                &paint,
                params.transform.pre_translate(gx, gy),
                params.clip,
            );
        }
    }
}

fn blit_glyph(
    pixels: &mut [PremultipliedColorU8],
    pw: u32,
    ph: u32,
    data: &[u8],
    gw: u32,
    gh: u32,
    x: i32,
    y: i32,
    r: u8,
    g: u8,
    b: u8,
    alpha_lut: &[u8; 256],
    clip: Option<&Mask>,
    clip_w: u32,
    clip_h: u32,
) {
    let (w, h) = (gw as i32, gh as i32);

    for row in 0..h {
        let target_y = y + row;
        if target_y < 0 || target_y >= ph as i32 {
            continue;
        }
        let src_offset = (row * w) as usize;

        for col in 0..w {
            let target_x = x + col;
            if target_x < 0 || target_x >= pw as i32 {
                continue;
            }

            let mut alpha = alpha_lut[data[src_offset + col as usize] as usize];
            if let Some(mask) = clip {
                let tx = target_x as u32;
                let ty = target_y as u32;
                if tx >= clip_w || ty >= clip_h {
                    continue;
                }
                // Modulate by the mask instead of thresholding so clipped
                // glyph edges fade like every other primitive.
                let coverage = mask.data()[(ty * clip_w + tx) as usize] as f32 / 255.0;
                alpha = (alpha as f32 * coverage).round() as u8;
            }

            if alpha == 0 {
                continue;
            }

            let idx = (target_y as usize * pw as usize) + target_x as usize;
            blend_source_over(&mut pixels[idx], r, g, b, alpha);
        }
    }
}

/// Blits glyph coverage at a fractional pixel offset.
///
/// The integer path above rounds the glyph origin, so an animation sweeping a
/// translation moves text in whole-pixel steps — and worse, each glyph's
/// fractional remainder differs (advances accumulate fractionally), so glyphs
/// pop across pixel boundaries at different times and the spacing visibly
/// boils. This distributes each coverage texel across its neighbours by the
/// fractional remainder instead: the run glides smoothly, and total ink is
/// conserved (the four weights sum to one), so nothing brightens or dims
/// mid-flight. This is the standard subpixel-positioning approximation —
/// shifting resampled coverage rather than re-rasterizing per offset — and it
/// keeps the cached integer-raster glyph bitmaps untouched.
#[allow(clippy::too_many_arguments)]
fn blit_glyph_fractional(
    pixels: &mut [PremultipliedColorU8],
    pw: u32,
    ph: u32,
    data: &[u8],
    gw: u32,
    gh: u32,
    x: i32,
    y: i32,
    fx: f32,
    fy: f32,
    r: u8,
    g: u8,
    b: u8,
    alpha_lut: &[u8; 256],
    clip: Option<&Mask>,
    clip_w: u32,
    clip_h: u32,
    scratch: &mut Vec<f32>,
) {
    let (w, h) = (gw as usize, gh as usize);
    debug_assert!(data.len() >= w * h, "glyph coverage smaller than its bitmap");
    // One extra row and column: a shifted texel spills over both far edges.
    let (sw, sh) = (w + 1, h + 1);
    scratch.clear();
    scratch.resize(sw * sh, 0.0);

    let (wx0, wy0) = (1.0 - fx, 1.0 - fy);
    for row in 0..h {
        for col in 0..w {
            let c = data[row * w + col] as f32;
            if c == 0.0 {
                continue;
            }
            let base = row * sw + col;
            // Weights sum to one per texel, so shifting conserves ink exactly
            // (up to the final rounding, shared with the integer path).
            scratch[base] += c * wx0 * wy0;
            scratch[base + 1] += c * fx * wy0;
            scratch[base + sw] += c * wx0 * fy;
            scratch[base + sw + 1] += c * fx * fy;
        }
    }

    for row in 0..sh {
        let target_y = y + row as i32;
        if target_y < 0 || target_y >= ph as i32 {
            continue;
        }
        for col in 0..sw {
            let target_x = x + col as i32;
            if target_x < 0 || target_x >= pw as i32 {
                continue;
            }
            let covered = scratch[row * sw + col].round().clamp(0.0, 255.0) as usize;
            let mut alpha = alpha_lut[covered];
            if let Some(mask) = clip {
                let tx = target_x as u32;
                let ty = target_y as u32;
                if tx >= clip_w || ty >= clip_h {
                    continue;
                }
                // Modulate by the mask instead of thresholding so clipped
                // glyph edges fade like every other primitive.
                let coverage = mask.data()[(ty * clip_w + tx) as usize] as f32 / 255.0;
                alpha = (alpha as f32 * coverage).round() as u8;
            }

            if alpha == 0 {
                continue;
            }

            let idx = (target_y as usize * pw as usize) + target_x as usize;
            blend_source_over(&mut pixels[idx], r, g, b, alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_lut() -> [u8; 256] {
        std::array::from_fn(|i| i as u8)
    }

    fn blank(w: u32, h: u32) -> Pixmap {
        Pixmap::new(w, h).expect("test pixmap allocates")
    }

    /// Zero fractions must reproduce the integer path exactly: the fast path
    /// stays bit-identical by construction, and this locks it in.
    #[test]
    fn zero_fractions_match_the_integer_blit() {
        let data: Vec<u8> = (0..20).map(|i| (i * 37 % 256) as u8).collect();
        let lut = identity_lut();

        let mut a = blank(16, 16);
        blit_glyph(
            a.pixels_mut(),
            16,
            16,
            &data,
            5,
            4,
            3,
            2,
            255,
            255,
            255,
            &lut,
            None,
            0,
            0,
        );

        let mut scratch = Vec::new();
        let mut b = blank(16, 16);
        blit_glyph_fractional(
            b.pixels_mut(),
            16,
            16,
            &data,
            5,
            4,
            3,
            2,
            0.0,
            0.0,
            255,
            255,
            255,
            &lut,
            None,
            0,
            0,
            &mut scratch,
        );

        assert_eq!(a.pixels(), b.pixels());
    }

    /// A half-pixel shift must split ink evenly, conserving the total: this is
    /// what turns per-pixel popping into a smooth glide.
    #[test]
    fn half_pixel_shift_splits_ink_evenly() {
        let lut = identity_lut();
        let mut scratch = Vec::new();
        let mut canvas = blank(16, 16);
        blit_glyph_fractional(
            canvas.pixels_mut(),
            16,
            16,
            &[255],
            1,
            1,
            4,
            4,
            0.5,
            0.0,
            255,
            255,
            255,
            &lut,
            None,
            0,
            0,
            &mut scratch,
        );

        let at = |x: usize, y: usize| canvas.pixels()[y * 16 + x].alpha();
        assert_eq!(at(4, 4), 128);
        assert_eq!(at(5, 4), 128);
        assert_eq!(at(4, 4) as u32 + at(5, 4) as u32, 256);
    }

    /// Shifting must conserve ink across a whole glyph: the sum of blended
    /// alpha is identical at integer and fractional origins (up to rounding),
    /// so text neither brightens nor dims mid-animation.
    #[test]
    fn fractional_shift_conserves_total_ink() {
        let data: Vec<u8> = (0..48).map(|i| (i * 53 % 256) as u8).collect();
        let lut = identity_lut();

        let total = |pixels: &[PremultipliedColorU8]| -> u32 {
            pixels.iter().map(|p| p.alpha() as u32).sum()
        };

        let mut a = blank(24, 24);
        blit_glyph(
            a.pixels_mut(),
            24,
            24,
            &data,
            8,
            6,
            5,
            5,
            255,
            255,
            255,
            &lut,
            None,
            0,
            0,
        );

        let mut scratch = Vec::new();
        let mut b = blank(24, 24);
        blit_glyph_fractional(
            b.pixels_mut(),
            24,
            24,
            &data,
            8,
            6,
            5,
            5,
            0.37,
            0.71,
            255,
            255,
            255,
            &lut,
            None,
            0,
            0,
            &mut scratch,
        );

        let (ta, tb) = (total(a.pixels()), total(b.pixels()));
        let diff = ta.abs_diff(tb);
        assert!(
            diff * 100 <= ta * 2,
            "ink must be conserved within rounding: integer sum {ta}, shifted sum {tb}"
        );
    }
}

fn blend_source_over(pixel: &mut PremultipliedColorU8, r: u8, g: u8, b: u8, a: u8) {
    let sa = a as f32 / 255.0;
    let inv_sa = 1.0 - sa;

    let dr = pixel.red() as f32;
    let dg = pixel.green() as f32;
    let db = pixel.blue() as f32;
    let da = pixel.alpha() as f32;

    let out_r = ((r as f32 * sa) + (dr * inv_sa)).round().min(255.0) as u8;
    let out_g = ((g as f32 * sa) + (dg * inv_sa)).round().min(255.0) as u8;
    let out_b = ((b as f32 * sa) + (db * inv_sa)).round().min(255.0) as u8;
    let out_a = ((a as f32) + (da * inv_sa)).round().min(255.0) as u8;

    if let Some(p) = PremultipliedColorU8::from_rgba(out_r, out_g, out_b, out_a) {
        *pixel = p;
    }
}
