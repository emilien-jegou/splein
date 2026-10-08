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

/// Display gamma assumed for coverage-to-alpha conversion.
///
/// Coverage masks are linear-light fractions, but compositing happens in
/// sRGB-encoded space. Blending raw coverage as alpha darkens mid-tone fringe
/// pixels (stems render heavier than specified). The LUT below folds the
/// transfer function in: `alpha = 1 - (1 - coverage)^(1/gamma)`, so a
/// half-covered pixel blends to the sRGB encoding of half light instead of
/// half ink. Monotonic, so threshold tests and bounding boxes are unaffected.
pub const COVERAGE_GAMMA: f32 = 2.2;

/// Gamma table at full opacity, computed once: 256 `powf` calls per `DrawText`
/// showed up as a real cost with many small runs on screen, so the curve is
/// process-wide and each run only folds in its scalar opacity (integer math).
static GAMMA_LUT: std::sync::LazyLock<[u8; 256]> = std::sync::LazyLock::new(|| {
    let inv_gamma = 1.0 / COVERAGE_GAMMA;
    std::array::from_fn(|i| {
        let linear = 1.0 - (1.0 - i as f32 / 255.0).powf(inv_gamma);
        (linear * 255.0).round() as u8
    })
});

/// Subpixel X phase bins per pixel: the pen quantizes to quarters and the
/// rasterizer produces one coverage variant per bin (see `raster_glyph`).
/// Animation cross-fades adjacent bins (see `blend_phase_coverages`), so
/// horizontal motion stays continuous while every source bitmap is
/// phase-correct.
pub const PHASE_BINS: u8 = 4;

/// Subpixel Y phase bins per pixel: baselines snap to the nearest eighth.
/// Eighth-pixel vertical steps hide in motion as whole-pixel ones never do,
/// while static text keeps a single crisp bitmap (blending Y measurably
/// softened it: the Y fraction is almost never exactly zero).
pub const PHASE_BINS_Y: u8 = 8;

/// Split a pen position into an integer base plus the fractional position
/// inside the pixel, in quarter-pixel units: `frac4` in [0, 4).
fn split_phase(pos: f32) -> (i32, f32) {
    let floor = pos.floor();
    (floor as i32, (pos - floor) * PHASE_BINS as f32)
}

/// Split a pen position into an integer base plus the fractional position
/// inside the pixel, in eighth-pixel units: `frac8` in [0, 8).
fn split_phase_y(pos: f32) -> (i32, f32) {
    let floor = pos.floor();
    (floor as i32, (pos - floor) * PHASE_BINS_Y as f32)
}

/// Blend two adjacent phase-bin coverages onto a common grid.
///
/// `a` is the bitmap for bin `q` placed at `(ax, ay)`, `b` the bitmap for the
/// next bin up placed at `(bx, by)` — same origin for bins 0..3, one pixel
/// right when wrapping from bin 3 to bin 0. Returns the union origin and size;
/// `out` is resized to hold the blended coverage (`a*w0 + b*w1`, rounded).
/// Out-of-range reads (empty bitmaps, ragged edges) contribute zero.
fn blend_phase_coverages(
    a: &[u8],
    aw: u32,
    ah: u32,
    ax: i32,
    ay: i32,
    w0: f32,
    b: &[u8],
    bw: u32,
    bh: u32,
    bx: i32,
    by: i32,
    w1: f32,
    out: &mut Vec<u8>,
) -> (i32, i32, u32, u32) {
    let x0 = ax.min(bx);
    let y0 = ay.min(by);
    let x1 = (ax + aw as i32).max(bx + bw as i32);
    let y1 = (ay + ah as i32).max(by + bh as i32);
    let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    out.clear();
    out.resize(w * h, 0);
    if w == 0 || h == 0 {
        return (x0, y0, 0, 0);
    }
    let at = |data: &[u8], dw: u32, ox: i32, oy: i32, x: usize, y: usize| -> f32 {
        let lx = x as i32 + x0 - ox;
        let ly = y as i32 + y0 - oy;
        if lx < 0 || ly < 0 {
            return 0.0;
        }
        data.get(ly as usize * dw as usize + lx as usize)
            .copied()
            .unwrap_or(0) as f32
    };
    for y in 0..h {
        for x in 0..w {
            let v = at(a, aw, ax, ay, x, y) * w0 + at(b, bw, bx, by, x, y) * w1;
            out[y * w + x] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    (x0, y0, w as u32, h as u32)
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

    // Coverage-to-alpha tables for this run: small text composites raw
    // coverage (density carries grid-fit stems), display type folds the
    // gamma curve in. Threshold mirrors the hinter cutoff in aurora
    // (`SMALL_TEXT_HINT_CUTOFF_PX`): hinted bitmaps want linear density,
    // linear outlines want corrected fringes.
    let opacity_q = (total_opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    let linear_lut: [u8; 256] =
        std::array::from_fn(|i| ((i as u32 * opacity_q + 127) / 255) as u8);
    let gamma_lut: [u8; 256] =
        std::array::from_fn(|i| ((GAMMA_LUT[i] as u32 * opacity_q + 127) / 255) as u8);
    // Mask dimensions hoisted out of the per-pixel loop below.
    let (clip_w, clip_h) = match params.clip {
        Some(mask) => (mask.width(), mask.height()),
        None => (0, 0),
    };

    // Reused staging buffers for glyph coverage. `raster_glyph` results are
    // copied out so blitting never borrows from the cache; `cov10` stages
    // the upper X endpoint, `blended` the combination.
    let mut cov00 = Vec::with_capacity(256);
    let mut cov10 = Vec::with_capacity(256);
    let mut blended = Vec::new();

    for line in &params.layout.lines {
        // X cross-fades the two adjacent quarter-pixel bins around the true
        // pen, so horizontal motion is continuous; Y snaps to the nearest
        // eighth (single crisp bitmap, 1/8px steps like Chrome/Skia).
        // Fully blending Y as well softened static text measurably (the Y
        // fraction is almost never exactly zero), while eighth-pixel Y
        // steps hide in motion as whole-pixel ones never do.
        //
        // Small text composites linear coverage, display type gamma-corrected
        // (see the LUTs above); the line's own size picks the table.
        let alpha_lut: &[u8; 256] = if line.font_size <= aurora::text::SMALL_TEXT_HINT_CUTOFF_PX {
            &linear_lut
        } else {
            &gamma_lut
        };
        for g in &line.glyphs {
            let mut pt = SkiaPoint::from_xy(
                params.origin.x + line.align_offset + g.point.x,
                params.origin.y + g.point.y,
            );
            params.transform.map_point(&mut pt);
            let (base_x, frac4x) = split_phase(pt.x);
            let binx = frac4x.floor().min(3.0) as u8;
            let tx = frac4x - binx as f32;
            // Y snaps to the nearest eighth (see PHASE_BINS_Y): single crisp
            // bitmap, 1/8px steps in motion.
            let (ybase, yfrac8) = split_phase_y(pt.y);
            let yq = yfrac8.round() as i32;
            let (base_y, biny) = (
                ybase + yq.div_euclid(PHASE_BINS_Y as i32),
                yq.rem_euclid(PHASE_BINS_Y as i32) as u8,
            );
            // Upper X bin wraps to bin 0 of the next pixel past 3/4.
            let binx1 = if binx < 3 { binx + 1 } else { 0 };
            let shiftx = i32::from(binx == 3);

            // Fetch the base corner always; the upper X corner only when
            // the pen is actually fractional on that axis.
            let mut p00: Option<(i32, i32, u32, u32)> = None;
            text_ctx.raster_glyph(
                g.cache_key,
                binx | (biny << 2),
                |img_opt| {
                    if let Some(img) = img_opt {
                        cov00.clear();
                        cov00.extend_from_slice(img.data);
                        p00 = Some((
                            base_x + img.left,
                            base_y - img.top,
                            img.width,
                            img.height,
                        ));
                    }
                },
            );
            let Some((x00, y00, w00, h00)) = p00 else {
                continue;
            };
            // Exact X bin hit: single crisp blit, no blend.
            if tx == 0.0 {
                blit_glyph(
                    pixels, pw, ph, &cov00, w00, h00, x00, y00,
                    ink_r, ink_g, ink_b, &alpha_lut, params.clip, clip_w, clip_h,
                );
                continue;
            }
            let mut p10: Option<(i32, i32, u32, u32)> = None;
            text_ctx.raster_glyph(
                g.cache_key,
                binx1 | (biny << 2),
                |img_opt| {
                    if let Some(img) = img_opt {
                        cov10.clear();
                        cov10.extend_from_slice(img.data);
                        p10 = Some((
                            base_x + shiftx + img.left,
                            base_y - img.top,
                            img.width,
                            img.height,
                        ));
                    }
                },
            );
            let Some((x10, y10, w10, h10)) = p10 else {
                // Upper variant missing (evicted/oversized): hold the lower
                // bin for this frame rather than dropping the glyph.
                blit_glyph(
                    pixels, pw, ph, &cov00, w00, h00, x00, y00,
                    ink_r, ink_g, ink_b, &alpha_lut, params.clip, clip_w, clip_h,
                );
                continue;
            };
            let (ux, uy, uw, uh) = blend_phase_coverages(
                &cov00, w00, h00, x00, y00, 1.0 - tx,
                &cov10, w10, h10, x10, y10, tx,
                &mut blended,
            );
            blit_glyph(
                pixels, pw, ph, &blended, uw, uh, ux, uy,
                ink_r, ink_g, ink_b, &alpha_lut, params.clip, clip_w, clip_h,
            );
        }
    }
}

/// Renders glyph runs through the backend so rotation and scale shape the
/// glyphs themselves. Each glyph becomes a tinted coverage pixmap drawn with
/// the full transform from its quarter-snapped origin: the engine carries
/// origin + transform, tiny-skia does the filtering and clipping.
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
            // Phase-matched bitmap per axis (X quarters, Y eighths): the
            // residual the backend filters is tiny, and the coverage itself
            // is rasterized at the right phase. Matches the aligned path.
            let pen_x = params.origin.x + line.align_offset + g.point.x;
            let (base_x, frac4x) = split_phase(pen_x);
            let binx = frac4x.floor().min(3.0) as u8;
            let pen_y = params.origin.y + g.point.y;
            let (ybase, yfrac8) = split_phase_y(pen_y);
            let yq = yfrac8.round() as i32;
            let (base_y, biny) = (
                ybase + yq.div_euclid(PHASE_BINS_Y as i32),
                yq.rem_euclid(PHASE_BINS_Y as i32) as u8,
            );

            let mut placed: Option<(f32, f32, u32, u32)> = None;
            text_ctx.raster_glyph(g.cache_key, binx | (biny << 2), |img_opt| {
                if let Some(img) = img_opt {
                    coverage.clear();
                    coverage.extend_from_slice(img.data);
                    placed = Some((
                        base_x as f32 + binx as f32 * 0.25 + img.left as f32,
                        base_y as f32 + biny as f32 * 0.125 - img.top as f32,
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
#[cfg(test)]
mod tests {
    use super::*;

    fn identity_lut() -> [u8; 256] {
        std::array::from_fn(|i| i as u8)
    }

    fn blank(w: u32, h: u32) -> Pixmap {
        Pixmap::new(w, h).expect("test pixmap allocates")
    }

    /// Phase split covers the pixel exactly once: base plus fraction
    /// reconstructs the pen, fraction in [0, 4).
    #[test]
    fn split_phase_covers_the_pixel() {
        assert_eq!(split_phase(4.0), (4, 0.0));
        assert_eq!(split_phase(4.5), (4, 2.0));
        let (base, frac4) = split_phase(-0.1);
        assert_eq!(base, -1);
        assert!((frac4 - 3.6).abs() < 1e-6, "frac4 = {frac4}");
        for hundredths in -200..200 {
            let pos = hundredths as f32 / 100.0;
            let (b, f) = split_phase(pos);
            assert!((0.0..4.0).contains(&f), "pos {pos}: frac4 {f}");
            assert!(
                ((b as f32 + f / 4.0) - pos).abs() < 1e-6,
                "pos {pos} reconstructs"
            );
        }
    }

    /// Blend endpoints: weight 1.0 reproduces the first bitmap, 0.0 the
    /// second, and the union origin covers both placements.
    #[test]
    fn blend_endpoints_reproduce_inputs() {
        let mut out = Vec::new();
        // t = 0 → a verbatim (b placed one right, must not leak in).
        let (x, y, w, h) = blend_phase_coverages(
            &[200], 1, 1, 5, 5, 1.0,
            &[100], 1, 1, 6, 5, 0.0,
            &mut out,
        );
        assert_eq!((x, y, w, h), (5, 5, 2, 1));
        assert_eq!(out, vec![200, 0]);
        // t = 1 → b verbatim.
        let (x, y, w, h) = blend_phase_coverages(
            &[200], 1, 1, 5, 5, 0.0,
            &[100], 1, 1, 6, 5, 1.0,
            &mut out,
        );
        assert_eq!((x, y, w, h), (5, 5, 2, 1));
        assert_eq!(out, vec![0, 100]);
    }

    /// Blend midpoint: overlapping texels average, ragged edges contribute
    /// zero outside their own bitmap (wrap case: bin 3 at base, bin 0 at
    /// base + 1).
    #[test]
    fn blend_midpoint_averages_overlap() {
        let mut out = Vec::new();
        let (x, y, w, h) = blend_phase_coverages(
            &[200, 200], 2, 1, 5, 5, 0.5,
            &[100, 100], 2, 1, 6, 5, 0.5,
            &mut out,
        );
        assert_eq!((x, y, w, h), (5, 5, 3, 1));
        assert_eq!(out, vec![100, 150, 50]);
    }


    /// Integer blit sanity: full-coverage texel lands opaque ink, zero stays.
    #[test]
    fn integer_blit_paints_coverage_verbatim() {
        let lut = identity_lut();
        let mut canvas = blank(8, 8);
        blit_glyph(
            canvas.pixels_mut(),
            8,
            8,
            &[255, 0],
            2,
            1,
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
        let at = |x: usize, y: usize| canvas.pixels()[y * 8 + x].alpha();
        assert_eq!(at(3, 2), 255);
        assert_eq!(at(4, 2), 0);
        assert_eq!(at(2, 2), 0);
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
