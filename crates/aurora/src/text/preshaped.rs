// Single responsibility: Build-time pre-shaped text blobs (proc-macro output).
//
// A `PreShapedText` is typography computed on the build host: glyph advances,
// cluster mapping, and integer-snapped coverage bitmaps, serialized as a
// versioned byte blob the `pre_shaped!` macro embeds with `&[...]`. At runtime
// `TextContext::shape_pre_shaped` rehydrates it into a `TextLayout` and seeds
// the glyph bitmap cache — no parley shaping, no swash rasterization, no font
// blobs for covered strings.
//
// Documented tradeoff (not identical quality): bitmaps are rasterized at
// integer pen phase and glyph points are snapped to device pixels at build
// time. Advances stay exact, so measurement matches runtime shaping; edge
// coverage can differ by subpixel AA from phase-exact runtime raster — the
// same convention as hinted small text. Strings needing fractional
// animation or wrapping stay on the runtime path.
//
// Blob layout (all little-endian):
// `AURP` magic + u32 version + f32 size + f32 line_height + f32 total_w/h +
// u32 line count, then per line: u32 glyph count + f32
// width/height/baseline/font_size, then per glyph: u32 id + f32 x/y/advance
// + u32 cluster + i32 left/top + u32 w/h + w*h coverage bytes.

use crate::foundation::{IntrinsicSize, Point};
use crate::text::glyph::{GlyphKey, ShapedGlyph, ShapedLine};
use crate::text::layout::TextLayout;

const MAGIC: u32 = 0x41555250; // "AURP"
const VERSION: u32 = 1;

/// Top-bit tag mixed into synthetic glyph keys: marks pre-shaped origin for
/// debugging and future key-space partitioning. (It is NOT a collision
/// barrier — runtime Fx hashes are uniform, so any 64-bit key collides with
/// probability 2^-64 per pair either way; the tag just names the source.)
const KEY_NAMESPACE: u64 = 1 << 63;

/// Sentinel font-blob index for pre-shaped glyphs (no blob at runtime).
pub const PRESHAPED_NO_BLOB: usize = usize::MAX;

/// A decoded pre-shaped text blob borrowing its bytes.
#[derive(Clone, Debug)]
pub struct PreShapedText<'a> {
    /// Point size the text was shaped at.
    pub size: f32,
    /// Resolved line height in logical px.
    pub line_height: f32,
    /// Total laid-out size.
    pub total_size: IntrinsicSize,
    /// Raw blob (bitmaps decoded lazily per line below).
    blob: &'a [u8],
    line_table: Vec<(usize, usize)>,
}

struct Cursor<'a> {
    rest: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.rest.len() < n {
            return None;
        }
        let (h, t) = self.rest.split_at(n);
        self.rest = t;
        Some(h)
    }

    fn u32(&mut self) -> Option<u32> {
        self.bytes(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }

    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
}

impl<'a> PreShapedText<'a> {
    /// Parses a blob emitted by `pre_shaped!`. `None` on any mismatch
    /// (magic, version, truncation, absurd counts) — never partial state.
    pub fn from_bytes(blob: &'a [u8]) -> Option<Self> {
        let mut c = Cursor { rest: blob };
        if c.u32()? != MAGIC || c.u32()? != VERSION {
            return None;
        }
        let (size, line_height) = (c.f32()?, c.f32()?);
        let total_size = IntrinsicSize {
            width: c.f32()?,
            height: c.f32()?,
        };
        let lines = c.u32()?;
        if lines > 4096 {
            return None;
        }
        // Walk the blob once recording line spans (offsets into `blob`).
        // Full validation happens per line at decode; here we only need the
        // structure to be traversable.
        let mut line_table = Vec::with_capacity(lines.min(256) as usize);
        for _ in 0..lines {
            let start = blob.len() - c.rest.len();
            let glyphs = c.u32()?;
            if glyphs > 65536 {
                return None;
            }
            for _ in 0..4 {
                c.f32()?;
            }
            for _ in 0..glyphs {
                // Mirror the encode order exactly (id; x/y/advance;
                // cluster; left/top; w/h): same byte count either way, but
                // only the true order keeps the cursor aligned.
                c.u32()?;
                for _ in 0..3 {
                    c.f32()?;
                }
                c.u32()?;
                for _ in 0..2 {
                    c.i32()?;
                }
                let (w, h) = (c.u32()?, c.u32()?);
                if w > 512 || h > 512 {
                    return None;
                }
                let n = (w as usize).checked_mul(h as usize)?;
                c.bytes(n)?;
            }
            line_table.push((start, blob.len() - c.rest.len()));
        }
        if !c.rest.is_empty() {
            return None;
        }
        Some(Self {
            size,
            line_height,
            total_size,
            blob,
            line_table,
        })
    }

    /// Stable synthetic glyph key (top-bit tagged; 2^-64 collision odds).
    pub fn glyph_key(glyph_id: u32, size_bits: u32, left: i32, top: i32, w: u32, h: u32) -> GlyphKey {
        use std::hash::{Hash, Hasher};
        let mut s = rustc_hash::FxHasher::default();
        (glyph_id, size_bits, left, top, w, h).hash(&mut s);
        GlyphKey(s.finish() & !KEY_NAMESPACE | KEY_NAMESPACE)
    }

    /// Decodes line `index` into a shaped line plus its seedable bitmaps.
    pub fn decode_line(
        &self,
        index: usize,
    ) -> Option<(
        ShapedLine,
        Vec<(GlyphKey, i32, i32, u32, u32, &'a [u8])>,
    )> {
        let (start, _) = *self.line_table.get(index)?;
        let mut c = Cursor {
            rest: &self.blob[start..],
        };
        let glyphs = c.u32()? as usize;
        let (width, height, baseline, font_size) = (c.f32()?, c.f32()?, c.f32()?, c.f32()?);
        let mut out = Vec::with_capacity(glyphs.min(512));
        let mut bitmaps = Vec::with_capacity(glyphs.min(512));
        for _ in 0..glyphs {
            let glyph_id = c.u32()?;
            let (x, y, advance) = (c.f32()?, c.f32()?, c.f32()?);
            let cluster = c.u32()? as usize;
            let (left, top) = (c.i32()?, c.i32()?);
            let (w, h) = (c.u32()?, c.u32()?);
            let data = c.bytes((w as usize) * (h as usize))?;
            let key = Self::glyph_key(glyph_id, self.size.to_bits(), left, top, w, h);
            bitmaps.push((key, left, top, w, h, data));
            out.push(ShapedGlyph {
                glyph_id,
                point: Point::new(x, y),
                advance,
                cluster,
                cache_key: key,
                font_blob: PRESHAPED_NO_BLOB,
            });
        }
        Some((ShapedLine::new(out, width, height, baseline, font_size), bitmaps))
    }

    /// Line count.
    pub fn line_count(&self) -> usize {
        self.line_table.len()
    }

    /// Rehydrates the full layout (glyphs + lines; bitmap seeding is the
    /// caller's job — see `TextContext::shape_pre_shaped`).
    pub fn to_layout(&self) -> TextLayout {
        let mut lines = Vec::with_capacity(self.line_count());
        for i in 0..self.line_count() {
            if let Some((line, _)) = self.decode_line(i) {
                lines.push(line);
            }
        }
        TextLayout::new(lines, self.total_size)
    }

    /// All glyph bitmaps with their synthetic keys, for cache seeding.
    pub fn bitmaps(&self) -> Vec<(GlyphKey, i32, i32, u32, u32, &'a [u8])> {
        let mut all = Vec::new();
        for i in 0..self.line_count() {
            if let Some((_, mut b)) = self.decode_line(i) {
                all.append(&mut b);
            }
        }
        all
    }
}

/// Builds a blob from shaped data (used by the proc macro on the build host
/// and by tests). Points are snapped to integer device px here — the documented
/// integer-pen contract.
#[allow(clippy::too_many_arguments)]
pub fn encode_blob(
    size: f32,
    line_height: f32,
    total: IntrinsicSize,
    lines: &[(
        Vec<(u32, f32, f32, f32, usize, i32, i32, u32, u32, Vec<u8>)>,
        f32,
        f32,
        f32,
        f32,
    )],
) -> Vec<u8> {
    fn u32le(v: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn f32le(v: f32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    let mut out = Vec::new();
    u32le(MAGIC, &mut out);
    u32le(VERSION, &mut out);
    f32le(size, &mut out);
    f32le(line_height, &mut out);
    f32le(total.width, &mut out);
    f32le(total.height, &mut out);
    u32le(lines.len() as u32, &mut out);
    for (glyphs, width, height, baseline, font_size) in lines {
        u32le(glyphs.len() as u32, &mut out);
        f32le(*width, &mut out);
        f32le(*height, &mut out);
        f32le(*baseline, &mut out);
        f32le(*font_size, &mut out);
        for (id, x, y, adv, cluster, left, top, w, h, data) in glyphs {
            u32le(*id, &mut out);
            f32le(x.round(), &mut out);
            f32le(y.round(), &mut out);
            f32le(*adv, &mut out);
            u32le(*cluster as u32, &mut out);
            u32le(*left as u32, &mut out);
            u32le(*top as u32, &mut out);
            u32le(*w, &mut out);
            u32le(*h, &mut out);
            out.extend_from_slice(data);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::IntrinsicSize;

    fn sample_blob() -> Vec<u8> {
        encode_blob(
            16.0,
            20.0,
            IntrinsicSize {
                width: 100.0,
                height: 20.0,
            },
            &[(
                vec![
                    (7u32, 1.2, 3.6, 9.5, 0usize, -1i32, -12i32, 8u32, 14u32, vec![128u8; 112]),
                    (9u32, 10.7, 3.6, 8.0, 1usize, 0i32, -11i32, 7u32, 13u32, vec![200u8; 91]),
                ],
                20.0,
                20.0,
                15.0,
                16.0,
            )],
        )
    }

    #[test]
    fn round_trip_preserves_metrics() {
        let blob = sample_blob();
        let pre = PreShapedText::from_bytes(&blob).expect("decodes");
        assert_eq!(pre.size, 16.0);
        assert_eq!(pre.line_height, 20.0);
        assert_eq!(pre.line_count(), 1);
        let layout = pre.to_layout();
        assert_eq!(layout.lines.len(), 1);
        let line = &layout.lines[0];
        assert_eq!(line.glyphs.len(), 2);
        // Points snapped to integer pins at encode time.
        assert_eq!(line.glyphs[0].point.x, 1.0);
        assert_eq!(line.glyphs[0].point.y, 4.0);
        // Advances stay exact.
        assert_eq!(line.glyphs[0].advance, 9.5);
        assert_eq!(line.glyphs[0].cluster, 0);
        assert_eq!(line.glyphs[0].font_blob, PRESHAPED_NO_BLOB);
        assert_eq!(line.width, 20.0);
        assert_eq!(layout.total_size.width, 100.0);
        // Bitmaps decode with keys + bearings.
        let bm = pre.bitmaps();
        assert_eq!(bm.len(), 2);
        assert_eq!(bm[0].1, -1);
        assert_eq!(bm[0].2, -12);
        assert_eq!(bm[0].5.len(), 112);
        // Keys stable across decodes.
        let again = PreShapedText::from_bytes(&blob).expect("decodes");
        assert_eq!(pre.bitmaps()[0].0, again.bitmaps()[0].0);
    }

    #[test]
    fn corrupt_blobs_rejected() {
        let blob = sample_blob();
        assert!(PreShapedText::from_bytes(&[]).is_none());
        assert!(PreShapedText::from_bytes(&blob[..12]).is_none());
        let mut bad = blob.clone();
        bad[0] = 0xFF;
        assert!(PreShapedText::from_bytes(&bad).is_none());
        let mut trailing = blob.clone();
        trailing.push(0);
        assert!(PreShapedText::from_bytes(&trailing).is_none());
        let mut huge = blob.clone();
        // Corrupt the line count to 1M.
        huge[24..28].copy_from_slice(&1_000_000u32.to_le_bytes());
        assert!(PreShapedText::from_bytes(&huge).is_none());
    }
}
