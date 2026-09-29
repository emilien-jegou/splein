// Single responsibility: Low-level zero-allocation rasterization primitives for debug visual cues.

use crate::foundation::ResolvedRect;

/// Blends a transparent color tint into a bounded rectangle of pixels.
pub fn draw_tint(buffer: &mut [u32], rect: &ResolvedRect, rgb: u32, alpha: u8, stride: usize, height: usize) {
    let (x0, y0, x1, y1) = clamp_bounds(rect, stride, height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let inv_a = 255 - alpha as u32;
    let (tr, tg, tb) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);

    for y in y0..y1 {
        let row = y * stride;
        for x in x0..x1 {
            let px = buffer[row + x];
            let (pr, pg, pb) = ((px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF);
            let r = (tr * alpha as u32 + pr * inv_a) / 255;
            let g = (tg * alpha as u32 + pg * inv_a) / 255;
            let b = (tb * alpha as u32 + pb * inv_a) / 255;
            buffer[row + x] = (r << 16) | (g << 8) | b;
        }
    }
}

/// Draws an outline border wireframe with explicit pixel thickness.
pub fn draw_wireframe(buffer: &mut [u32], rect: &ResolvedRect, rgb: u32, thickness: usize, stride: usize, height: usize) {
    let (x0, y0, x1, y1) = clamp_bounds(rect, stride, height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let t = thickness.min((x1 - x0) / 2).min((y1 - y0) / 2).max(1);

    for b in 0..t {
        for x in x0..x1 {
            buffer[(y0 + b) * stride + x] = rgb;
            buffer[(y1 - 1 - b) * stride + x] = rgb;
        }
        for y in y0..y1 {
            buffer[y * stride + (x0 + b)] = rgb;
            buffer[y * stride + (x1 - 1 - b)] = rgb;
        }
    }
}

/// Draws a nested double-line border for layout boundary isolation.
pub fn draw_double_border(buffer: &mut [u32], rect: &ResolvedRect, rgb: u32, stride: usize, height: usize) {
    draw_wireframe(buffer, rect, rgb, 1, stride, height);
    let inner = rect.expand(-3.0);
    if !inner.is_empty() {
        draw_wireframe(buffer, &inner, rgb, 1, stride, height);
    }
}

/// Draws a dashed wireframe border indicating removed or transient nodes.
pub fn draw_dashed_wireframe(buffer: &mut [u32], rect: &ResolvedRect, rgb: u32, stride: usize, height: usize) {
    let (x0, y0, x1, y1) = clamp_bounds(rect, stride, height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for x in x0..x1 {
        if (x / 4) % 2 == 0 {
            buffer[y0 * stride + x] = rgb;
            buffer[(y1 - 1) * stride + x] = rgb;
        }
    }
    for y in y0..y1 {
        if (y / 4) % 2 == 0 {
            buffer[y * stride + x0] = rgb;
            buffer[y * stride + (x1 - 1)] = rgb;
        }
    }
}

/// Clamps floating point bounds outward into valid scanline and pixel bounds.
pub fn clamp_bounds(rect: &ResolvedRect, stride: usize, height: usize) -> (usize, usize, usize, usize) {
    let x0 = (rect.x.floor() as i32).max(0) as usize;
    let y0 = (rect.y.floor() as i32).max(0) as usize;
    let x1 = (rect.right().ceil() as usize).min(stride);
    let y1 = (rect.bottom().ceil() as usize).min(height);
    (x0, y0, x1, y1)
}
