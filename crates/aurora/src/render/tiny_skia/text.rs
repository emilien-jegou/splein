// Single responsibility: Glyph bitmap Porter-Duff alpha compositing via TinySkia.

use tiny_skia::{Mask, PixmapMut, Point as SkiaPoint, PremultipliedColorU8, Transform};
use crate::foundation::{Color, Point};
use crate::text::{GlyphBitmap, TextContext, TextLayout};

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
pub fn render_text(
    pixmap: &mut PixmapMut,
    text_ctx: &TextContext,
    params: TextRenderParams,
) {
    if params.layout.lines.is_empty() { return; }

    let pw = pixmap.width();
    let ph = pixmap.height();
    let pixels = pixmap.pixels_mut();

    let ink_r = (params.color.r * 255.0) as u8;
    let ink_g = (params.color.g * 255.0) as u8;
    let ink_b = (params.color.b * 255.0) as u8;
    let total_opacity = params.opacity * params.color.a;

    for line in &params.layout.lines {
        for g in &line.glyphs {
            text_ctx.raster_glyph(g.cache_key, |img_opt| {
                if let Some(img) = img_opt {
                    let mut pt = SkiaPoint::from_xy(params.origin.x + g.point.x, params.origin.y + g.point.y);
                    params.transform.map_point(&mut pt);

                    let start_x = pt.x.round() as i32 + img.left;
                    let start_y = pt.y.round() as i32 - img.top;

                    blit_glyph(pixels, pw, ph, &img, start_x, start_y, ink_r, ink_g, ink_b, total_opacity, params.clip);
                }
            });
        }
    }
}

fn blit_glyph(
    pixels: &mut [PremultipliedColorU8],
    pw: u32,
    ph: u32,
    img: &GlyphBitmap,
    x: i32,
    y: i32,
    r: u8,
    g: u8,
    b: u8,
    opacity: f32,
    clip: Option<&Mask>,
) {
    let w = img.width as i32;
    let h = img.height as i32;

    for row in 0..h {
        let target_y = y + row;
        if target_y < 0 || target_y >= ph as i32 { continue; }
        let src_offset = (row * w) as usize;

        for col in 0..w {
            let target_x = x + col;
            if target_x < 0 || target_x >= pw as i32 { continue; }

            if let Some(mask) = clip {
                let mw = mask.width();
                let mh = mask.height();
                let tx = target_x as u32;
                let ty = target_y as u32;
                if tx >= mw || ty >= mh || mask.data()[(ty * mw + tx) as usize] == 0 {
                    continue;
                }
            }

            let alpha = (img.data[src_offset + col as usize] as f32 * opacity) as u8;
            if alpha == 0 { continue; }

            let idx = (target_y as usize * pw as usize) + target_x as usize;
            blend_source_over(&mut pixels[idx], r, g, b, alpha);
        }
    }
}

fn blend_source_over(pixel: &mut PremultipliedColorU8, r: u8, g: u8, b: u8, a: u8) {
    let sa = a as f32 / 255.0;
    let inv_sa = 1.0 - sa;

    let dr = pixel.red() as f32;
    let dg = pixel.green() as f32;
    let db = pixel.blue() as f32;
    let da = pixel.alpha() as f32;

    let out_r = ((r as f32 * sa) + (dr * inv_sa)).min(255.0) as u8;
    let out_g = ((g as f32 * sa) + (dg * inv_sa)).min(255.0) as u8;
    let out_b = ((b as f32 * sa) + (db * inv_sa)).min(255.0) as u8;
    let out_a = ((a as f32) + (da * inv_sa)).min(255.0) as u8;

    if let Some(p) = PremultipliedColorU8::from_rgba(out_r, out_g, out_b, out_a) {
        *pixel = p;
    }
}
