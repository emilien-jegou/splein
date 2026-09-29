// Single responsibility: High-throughput SIMD-accelerated pixel format conversion kernels (RGBA <-> BGRA).

/// Swizzles a slice of premultiplied RGBA pixels (TinySkia) to 0RGB/BGRA (Display surface).
/// Automatically dispatches to AVX2, SSSE3, ARM NEON, or unaligned fallback with zero pointer-alignment restrictions.
#[inline(always)]
pub fn swizzle_rgba_to_bgra(dst: &mut [u32], src: &[u32]) {
    let len = dst.len().min(src.len());
    if len == 0 {
        return;
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe {
                return swizzle_avx2(dst, src, len);
            }
        }
        if is_x86_feature_detected!("ssse3") {
            unsafe {
                return swizzle_ssse3(dst, src, len);
            }
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            return swizzle_neon(dst, src, len);
        }
    }

    #[allow(unreachable_code)]
    swizzle_scalar(dst, src, len);
}

// -----------------------------------------------------------------------------
// AVX2 Kernel: 8 pixels (32 bytes / 256 bits) per cycle with unaligned load/store
// -----------------------------------------------------------------------------
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn swizzle_avx2(dst: &mut [u32], src: &[u32], len: usize) {
    use std::arch::x86_64::*;

    let chunks = len / 8;
    let remainder = len % 8;

    // Swap Byte 0 and Byte 2 across each 4-byte pixel: [2, 1, 0, 3]
    let mask = _mm256_setr_epi8(
        2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15, 2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11,
        14, 13, 12, 15,
    );

    let mut src_ptr = src.as_ptr() as *const __m256i;
    let mut dst_ptr = dst.as_mut_ptr() as *mut __m256i;

    for _ in 0..chunks {
        let pixels = _mm256_loadu_si256(src_ptr);
        let swizzled = _mm256_shuffle_epi8(pixels, mask);
        _mm256_storeu_si256(dst_ptr, swizzled);

        src_ptr = src_ptr.add(1);
        dst_ptr = dst_ptr.add(1);
    }

    if remainder > 0 {
        let offset = chunks * 8;
        swizzle_scalar(&mut dst[offset..len], &src[offset..len], remainder);
    }
}

// -----------------------------------------------------------------------------
// SSSE3 Kernel: 4 pixels (16 bytes / 128 bits) per cycle with unaligned load/store
// -----------------------------------------------------------------------------
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn swizzle_ssse3(dst: &mut [u32], src: &[u32], len: usize) {
    use std::arch::x86_64::*;

    let chunks = len / 4;
    let remainder = len % 4;

    let mask = _mm_setr_epi8(2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15);

    let mut src_ptr = src.as_ptr() as *const __m128i;
    let mut dst_ptr = dst.as_mut_ptr() as *mut __m128i;

    for _ in 0..chunks {
        let pixels = _mm_loadu_si128(src_ptr);
        let swizzled = _mm_shuffle_epi8(pixels, mask);
        _mm_storeu_si128(dst_ptr, swizzled);

        src_ptr = src_ptr.add(1);
        dst_ptr = dst_ptr.add(1);
    }

    if remainder > 0 {
        let offset = chunks * 4;
        swizzle_scalar(&mut dst[offset..len], &src[offset..len], remainder);
    }
}

// -----------------------------------------------------------------------------
// ARM NEON Kernel (Apple Silicon / aarch64): 4 pixels (128 bits) per cycle
// -----------------------------------------------------------------------------
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn swizzle_neon(dst: &mut [u32], src: &[u32], len: usize) {
    use std::arch::aarch64::*;

    let chunks = len / 4;
    let remainder = len % 4;

    let mask = vld1q_u8([2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15].as_ptr());

    let mut src_ptr = src.as_ptr() as *const u8;
    let mut dst_ptr = dst.as_mut_ptr() as *mut u8;

    for _ in 0..chunks {
        let pixels = vld1q_u8(src_ptr);
        let swizzled = vqtbl1q_u8(pixels, mask);
        vst1q_u8(dst_ptr, swizzled);

        src_ptr = src_ptr.add(16);
        dst_ptr = dst_ptr.add(16);
    }

    if remainder > 0 {
        let offset = chunks * 4;
        swizzle_scalar(&mut dst[offset..len], &src[offset..len], remainder);
    }
}

// -----------------------------------------------------------------------------
// Safe Scalar / Tail Swizzler (Zero alignment restrictions)
// -----------------------------------------------------------------------------
#[inline(always)]
fn swizzle_scalar(dst: &mut [u32], src: &[u32], len: usize) {
    for (d, &px) in dst[..len].iter_mut().zip(src[..len].iter()) {
        *d = (px & 0xFF00FF00) | ((px & 0x000000FF) << 16) | ((px & 0x00FF0000) >> 16);
    }
}
