#[inline]
pub fn box_blur1d(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 { dst.copy_from_slice(src); return; }
    let k = 2*r + 1;
    // horizontal
    for y in 0..h {
        let mut sum = 0u32;
        for x in 0..=(2*r).min(w-1) {
            sum += src[y*w + x] as u32;
        }
        for x in 0..w {
            if x + r + 1 < w { sum += src[y*w + x + r + 1] as u32; }
            if x >= r { sum = sum.saturating_sub(src[y*w + x - r] as u32); }
            dst[y*w + x] = ((sum as u64) / (k as u64)) as u8;
        }
    }
    // vertical
    let mut tmp = vec![0u8; w*h];
    tmp.copy_from_slice(dst);
    for x in 0..w {
        let mut sum = 0u32;
        for y in 0..=(2*r).min(h-1) {
            sum += tmp[y*w + x] as u32;
        }
        for y in 0..h {
            if y + r + 1 < h { sum += tmp[(y + r + 1)*w + x] as u32; }
            if y >= r { sum = sum.saturating_sub(tmp[(y - r)*w + x] as u32); }
            dst[y*w + x] = ((sum as u64) / (k as u64)) as u8;
        }
    }
}
