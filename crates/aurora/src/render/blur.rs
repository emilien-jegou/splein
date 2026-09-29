// Single responsibility: Cache-aligned O(1) integer fixed-point sliding-window box blur.

pub fn box_blur_alpha(src: &[u8], dst: &mut [u8], temp: &mut [u8], w: usize, h: usize, radius: usize) {
    if radius == 0 || w == 0 || h == 0 {
        dst.copy_from_slice(src);
        return;
    }

    box_blur_row_pass(src, temp, w, h, radius);
    box_blur_col_pass(temp, dst, w, h, radius);
}

fn box_blur_row_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let window_size = (r * 2 + 1) as u32;
    let inv_fixed = ((1 << 16) + (window_size / 2)) / window_size;

    for y in 0..h {
        let row = y * w;
        let mut acc = (src[row] as u32) * (r as u32 + 1);

        for x in 0..r.min(w) {
            acc += src[row + x] as u32;
        }

        for x in 0..w {
            let add_x = (x + r + 1).min(w - 1);
            let sub_x = x.saturating_sub(r);

            acc += src[row + add_x] as u32;
            acc -= src[row + sub_x] as u32;

            dst[row + x] = ((acc * inv_fixed) >> 16) as u8;
        }
    }
}

fn box_blur_col_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let window_size = (r * 2 + 1) as u32;
    let inv_fixed = ((1 << 16) + (window_size / 2)) / window_size;

    for x in 0..w {
        let mut acc = (src[x] as u32) * (r as u32 + 1);

        for y in 0..r.min(h) {
            acc += src[y * w + x] as u32;
        }

        for y in 0..h {
            let add_y = (y + r + 1).min(h - 1);
            let sub_y = y.saturating_sub(r);

            acc += src[add_y * w + x] as u32;
            acc -= src[sub_y * w + x] as u32;

            dst[y * w + x] = ((acc * inv_fixed) >> 16) as u8;
        }
    }
}
