// Single responsibility: Cache-aligned O(1) integer fixed-point sliding-window box blur.

pub fn box_blur_alpha(
    src: &[u8],
    dst: &mut [u8],
    temp: &mut [u8],
    w: usize,
    h: usize,
    radius: usize,
) {
    if radius == 0 || w == 0 || h == 0 {
        dst.copy_from_slice(src);
        return;
    }

    box_blur_row_pass(src, temp, w, h, radius);
    box_blur_col_pass(temp, dst, w, h, radius);
}

/// Box blur followed by a falloff remap of the blurred coverage.
///
/// Runs two box iterations: the first converges toward Gaussian (smooth
/// tails, no rim kink for Mach bands to catch), the second attains
/// sigma ≈ 1.15r with ±2r support. Callers size `radius` for that: a CSS
/// blur R maps to r = R/2 (sigma ≈ 0.58R, support ±R). The LUT comes from
/// [`aurora_foundation::FalloffCurve::lut`]; callers that render multiple
/// layers of one shadow compute it once per distinct curve.
pub fn box_blur_alpha_with_falloff(
    src: &[u8],
    dst: &mut [u8],
    temp: &mut [u8],
    w: usize,
    h: usize,
    radius: usize,
    falloff_lut: &[u8; 256],
) {
    box_blur_alpha(src, dst, temp, w, h, radius);
    if radius != 0 && w != 0 && h != 0 {
        box_blur_row_pass(dst, temp, w, h, radius);
        box_blur_col_pass(temp, dst, w, h, radius);
    }
    for v in dst.iter_mut() {
        *v = falloff_lut[*v as usize];
    }
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
            // Window for x is [x-r, x+r]: entering is x+r, leaving is x-r-1.
            // (x+r+1 / x-r shifts the kernel a pixel and miscounts edges.)
            let add_x = (x + r).min(w - 1);
            let sub_x = x.saturating_sub(r + 1);

            acc += src[row + add_x] as u32;
            acc -= src[row + sub_x] as u32;

            // Round to nearest so solid input reaches 255 instead of
            // truncating at 254 and leaving a 1-level seam step.
            dst[row + x] = ((acc * inv_fixed + 0x8000) >> 16) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blur_2d(src: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
        let (mut dst, mut tmp) = (vec![0u8; w * h], vec![0u8; w * h]);
        box_blur_alpha(src, &mut dst, &mut tmp, w, h, r);
        dst
    }

    /// Two iterations spread at most ±2r with mass conserved: the whole
    /// 255 lands inside the support band (fixed-point truncation costs a
    /// few levels, single tail pixels may read zero — hence mass, not
    /// per-pixel bounds, on the inside edge).
    #[test]
    fn impulse_support_matches_radius() {
        let (w, h, r) = (64usize, 64usize, 4usize);
        let mut src = vec![0u8; w * h];
        src[32 * w + 32] = 255;
        let out = blur_2d(&src, w, h, r);
        assert_eq!(out[32 * w + (32 - 2 * r - 1)], 0, "support leaks left");
        assert_eq!(out[32 * w + (32 + 2 * r + 1)], 0, "support leaks right");
        // Total mass over the 2D support square: truncating fixed point
        // eats a share, but catastrophic accumulation breakage lands ~0.
        let mut total = 0u32;
        for y in 32 - 2 * r..=32 + 2 * r {
            for x in 32 - 2 * r..=32 + 2 * r {
                total += out[y * w + x] as u32;
            }
        }
        assert!(total >= 60, "support must retain mass, got {total}");
        assert!(out[32 * w + 32] > 0, "peak missing");
    }

    /// Impulse response is exactly mirror-symmetric about the impulse in
    /// both axes. Any kernel shift breaks this.
    #[test]
    fn impulse_response_is_centered() {
        let (w, h, r) = (64usize, 64usize, 4usize);
        let mut src = vec![0u8; w * h];
        src[32 * w + 32] = 255;
        let out = blur_2d(&src, w, h, r);
        // Mirror about the impulse at 32, not the canvas center.
        for y in 1..h {
            let my = 64 - y;
            if my >= h {
                continue;
            }
            for x in 1..w {
                let mx = 64 - x;
                if mx >= w {
                    continue;
                }
                assert_eq!(out[y * w + x], out[y * w + mx], "x-asymmetry at ({x},{y})");
                assert_eq!(out[y * w + x], out[my * w + x], "y-asymmetry at ({x},{y})");
            }
        }
    }

    /// A step edge blurs antisymmetrically about its midpoint (up to
    /// fixed-point rounding): out[31-d] + out[32+d] == 255.
    #[test]
    fn step_edge_is_antisymmetric() {
        let (w, h, r) = (64usize, 8usize, 4usize);
        let mut src = vec![0u8; w * h];
        for y in 0..h {
            for x in 32..w {
                src[y * w + x] = 255;
            }
        }
        let out = blur_2d(&src, w, h, r);
        for d in 0..16 {
            let (a, b) = (out[4 * w + 31 - d] as i16, out[4 * w + 32 + d] as i16);
            assert!(
                (a + b - 255).abs() <= 3,
                "edge antisymmetry broken at d={d}: {a}+{b}"
            );
        }
    }

    /// Radius zero is a verbatim copy.
    #[test]
    fn zero_radius_copies() {
        let src: Vec<u8> = (0..64).map(|i| (i * 4) as u8).collect();
        let out = blur_2d(&src, 8, 8, 0);
        assert_eq!(out, src);
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
            // Window for y is [y-r, y+r]: entering is y+r, leaving is y-r-1.
            let add_y = (y + r).min(h - 1);
            let sub_y = y.saturating_sub(r + 1);

            acc += src[add_y * w + x] as u32;
            acc -= src[sub_y * w + x] as u32;

            // Round to nearest (see row pass).
            dst[y * w + x] = ((acc * inv_fixed + 0x8000) >> 16) as u8;
        }
    }
}
