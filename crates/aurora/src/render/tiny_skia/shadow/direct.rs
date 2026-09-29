// Single responsibility: Direct Gaussian blur rasterizer for small or inset soft shadows.

use tiny_skia::{Mask, Pixmap, PixmapMut, PixmapPaint, Transform};
use crate::foundation::{Radius, ResolvedRect, Shadow};
use crate::render::blur::box_blur_alpha;

/// Renders fallback direct blur without 9-patch subdivision for small or inset shadows.
pub fn render_direct_blur(
    scratch_a: &mut Vec<u8>,
    scratch_b: &mut Vec<u8>,
    scratch_temp: &mut Vec<u8>,
    pixmap: &mut PixmapMut,
    rect: &ResolvedRect,
    radius: f32,
    shadow: &Shadow,
    opacity: f32,
    transform: Transform,
    clip: Option<&Mask>,
) {
    let scale = if shadow.blur >= 16.0 { 0.5f32 } else { 1.0f32 };
    let expansion = (shadow.blur * 3.0 + shadow.offset_x.abs().max(shadow.offset_y.abs())) * scale;
    let bounds = ResolvedRect::new(
        (rect.x * scale) - expansion + (shadow.offset_x * scale),
        (rect.y * scale) - expansion + (shadow.offset_y * scale),
        (rect.width * scale) + expansion * 2.0,
        (rect.height * scale) + expansion * 2.0,
    );

    let (bw, bh) = ((bounds.width.ceil() as u32).max(1), (bounds.height.ceil() as u32).max(1));
    let buf_len = (bw * bh) as usize;
    scratch_a.resize(buf_len, 0); scratch_b.resize(buf_len, 0); scratch_temp.resize(buf_len, 0);
    scratch_a.fill(0); scratch_b.fill(0);

    let blur_r = ((shadow.blur * scale).round() as usize).max(1);
    let scaled_rect = ResolvedRect::new(0.0, 0.0, rect.width * scale, rect.height * scale);
    let shape_path = crate::render::tiny_skia::path::build_rounded_path(&scaled_rect, Radius::Scalar(radius * scale));
    let Some(mut temp_pixmap) = Pixmap::new(bw, bh) else { return; };

    let draw_tx = Transform::from_translate(-bounds.x, -bounds.y).post_translate(shadow.offset_x * scale, shadow.offset_y * scale);
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);

    temp_pixmap.fill_path(&shape_path, &paint, tiny_skia::FillRule::Winding, draw_tx, None);
    for (dst, src) in scratch_a.iter_mut().zip(temp_pixmap.data().chunks_exact(4)) { *dst = src[3]; }
    box_blur_alpha(scratch_a, scratch_b, scratch_temp, bw as usize, bh as usize, blur_r);

    let c = shadow.color;
    for (dst, &alpha) in temp_pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(scratch_b.iter()) {
        let a = alpha as f32 / 255.0;
        dst[0] = ((c.r * a) * 255.0) as u8;
        dst[1] = ((c.g * a) * 255.0) as u8;
        dst[2] = ((c.b * a) * 255.0) as u8;
        dst[3] = (a * 255.0) as u8;
    }

    let final_tx = transform.pre_translate(bounds.x / scale, bounds.y / scale).pre_scale(1.0 / scale, 1.0 / scale);
    pixmap.draw_pixmap(0, 0, temp_pixmap.as_ref(), &PixmapPaint { opacity, ..Default::default() }, final_tx, clip);
}
