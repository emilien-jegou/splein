
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

}

///
/// Runs two box iterations: the first converges toward Gaussian (smooth
/// tails, no rim kink for Mach bands to catch), the second attains
/// sigma ≈ 1.15r with ±2r support. Callers size `radius` for that: a CSS
/// [`crate::foundation::FalloffCurve::lut`]; callers that render multiple
    src: &[u8],
    dst: &mut [u8],
    temp: &mut [u8],
    w: usize,
    h: usize,
    radius: usize,
    falloff_lut: &[u8; 256],
) {
    if radius != 0 && w != 0 && h != 0 {
    }
    for v in dst.iter_mut() {
        *v = falloff_lut[*v as usize];
    }
}

    let window_size = (r * 2 + 1) as u32;
    let inv_fixed = ((1 << 16) + (window_size / 2)) / window_size;

    #[cfg(feature = "parallel")]
    if w > 0 && h > 0 && r > 0 && w * h >= 4096 {
        use rayon::prelude::*;
        (0..h).into_par_iter().for_each(|y| {
            let mut acc = (src[y * w] as u32) * (r as u32 + 1);
            for x in 0..r.min(w) {
                acc += src[y * w + x] as u32;
            }
            for x in 0..w {
                let add_x = (x + r).min(w - 1);
                let sub_x = x.saturating_sub(r + 1);
                acc += src[y * w + add_x] as u32;
                acc -= src[y * w + sub_x] as u32;
                let raw = dst.as_ptr() as usize;
                unsafe {
                    *(raw as *mut u8).add(y * w + x) =
                        ((acc * inv_fixed + 0x8000) >> 16) as u8;
                }
            }
        });
        return;
    }

    for y in 0..h {
        let row = y * w;
        let mut acc = (src[row] as u32) * (r as u32 + 1);

        for x in 0..r.min(w) {
            acc += src[row + x] as u32;
        }

        for x in 0..w {
            let add_x = (x + r).min(w - 1);
            let sub_x = x.saturating_sub(r + 1);

            acc += src[row + add_x] as u32;
            acc -= src[row + sub_x] as u32;

            dst[row + x] = ((acc * inv_fixed + 0x8000) >> 16) as u8;
        }
    }
}

    let window_size = (r * 2 + 1) as u32;
    let inv_fixed = ((1 << 16) + (window_size / 2)) / window_size;

    #[cfg(feature = "parallel")]
    if w > 1 && h > 0 && r > 0 && w * h >= 4096 {
        use rayon::prelude::*;

        let raw = dst.as_ptr() as usize;
        (0..w).into_par_iter().for_each(|x| {
            let mut acc = (src[x] as u32) * (r as u32 + 1);
            for y in 0..r.min(h) {
                acc += src[y * w + x] as u32;
            }
            for y in 0..h {
                let add_y = (y + r).min(h - 1);
                let sub_y = y.saturating_sub(r + 1);
                acc += src[add_y * w + x] as u32;
                acc -= src[sub_y * w + x] as u32;
                let ptr = raw as *mut u8;
                unsafe {
                    *ptr.add(y * w + x) =
                        ((acc * inv_fixed + 0x8000) >> 16) as u8;
                }
            }
        });
        return;
    }

    for x in 0..w {
        let mut acc = (src[x] as u32) * (r as u32 + 1);
        for y in 0..r.min(h) {
            acc += src[y * w + x] as u32;
        }
        for y in 0..h {
            let add_y = (y + r).min(h - 1);
            let sub_y = y.saturating_sub(r + 1);
            acc += src[add_y * w + x] as u32;
            acc -= src[sub_y * w + x] as u32;
            let raw = dst.as_ptr() as usize;
                unsafe {
                    *(raw as *mut u8).add(y * w + x) =
                        ((acc * inv_fixed + 0x8000) >> 16) as u8;
                }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

        let (mut dst, mut tmp) = (vec![0u8; w * h], vec![0u8; w * h]);
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
        assert_eq!(out, src);
    }
}
