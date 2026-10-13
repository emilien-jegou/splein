// Single responsibility: Thread-safe memoization cache for shaped typography layouts.

use crate::text::align::TextAlign;
use crate::text::fonts::{FontId, FontStyle};
use crate::text::layout::TextLayout;
use crate::text::overflow::TextOverflow;
use crate::text::shaper::TextShapeParams;
use crate::text::wrap::TextWrap;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

const MAX_CACHE_ENTRIES: usize = 2048;

/// Byte budget for retained shaped layouts [S2-4]: count caps alone admit
/// ~20 MiB of long paragraphs (2048 × ~10 KB); the byte cap bounds worst
/// case regardless of entry size. Eviction only costs re-shaping — layouts
/// are deterministic, so pixels never change, only cache-hit rate.
const MAX_CACHE_BYTES: usize = 8 << 20;

/// Rough retained size of one entry: content key + glyph vec + line boxes.
/// `ShapedGlyph` ≈ 40 B, `ShapedLine` ≈ 48 B + glyphs; over-counting slightly
/// is safe (earlier eviction, identical pixels).
fn entry_bytes(text_len: usize, layout: &TextLayout) -> usize {
    let mut bytes = text_len + 128;
    for line in &layout.lines {
        bytes += 48 + line.glyphs.len() * 40;
    }
    bytes
}

/// Cache identity granularity: layout routinely produces fractional widths
/// (spring values, fill division, window resize) that differ by far less
/// than a pixel. Quantizing to 1/16px is visually identical in shaping
/// output and turns per-frame jitter into cache hits instead of cold
/// shapes — an animation sweeping a width from 400.0 to 400.5 reuses one
/// entry rather than shaping dozens.
const QUANTIZE_STEPS_PER_PX: f32 = 16.0;

/// Identity bits for one float dimension: non-finite values share a bucket,
/// and ±0 collapse to +0 so sign jitter on zero never defeats the cache.
fn quantize_bits(v: f32) -> u32 {
    if !v.is_finite() {
        return u32::MAX;
    }
    let q = (v * QUANTIZE_STEPS_PER_PX).round();
    (if q == 0.0 { 0.0 } else { q }).to_bits()
}

/// Style and width identity shared by every string shaped the same way.
#[derive(Clone, PartialEq, Eq, Hash)]
struct StyleKey {
    font: Option<FontId>,
    family: Option<String>,
    style: FontStyle,
    size_bits: u32,
    weight: u16,
    variations: Vec<([u8; 4], u32)>,
    line_height_bits: u32,
    letter_spacing_bits: u32,
    align: TextAlign,
    wrap: TextWrap,
    overflow: TextOverflow,
    max_lines: Option<u32>,
    max_width_bits: u32,
}

impl StyleKey {
    /// Same style ignoring the wrap width: everything but `max_width_bits`.
    /// Used by `get_nearby` to find sibling buckets during resize drags.
    fn same_style(&self, other: &Self) -> bool {
        self.font == other.font
            && self.family == other.family
            && self.style == other.style
            && self.size_bits == other.size_bits
            && self.weight == other.weight
            && self.variations == other.variations
            && self.line_height_bits == other.line_height_bits
            && self.letter_spacing_bits == other.letter_spacing_bits
            && self.align == other.align
            && self.wrap == other.wrap
            && self.overflow == other.overflow
            && self.max_lines == other.max_lines
    }

    /// Builds a style identity from shaping parameters, ignoring the content string.
    ///
    /// Float dimensions are quantized (see `quantize_bits`): two layouts
    /// whose widths differ by less than 1/16px share the first shaped
    /// entry, which is what keeps animated/resizing text on the cache-hit
    /// path instead of cold-shaping every frame.
    fn from_params(params: &TextShapeParams) -> Self {
        Self {
            font: params.font,
            family: params.family.map(str::to_string),
            style: params.style,
            size_bits: quantize_bits(params.size),
            weight: params.weight,
            variations: params
                .variations
                .iter()
                .map(|v| (v.tag, v.value.to_bits()))
                .collect(),
            line_height_bits: quantize_bits(params.line_height),
            letter_spacing_bits: quantize_bits(params.letter_spacing),
            align: params.align,
            wrap: params.wrap,
            overflow: params.overflow,
            max_lines: params.max_lines,
            max_width_bits: quantize_bits(params.constraints.max_width),
        }
    }
}

/// Cached layout plus its last-use clock for LRU eviction.
struct Entry {
    layout: Arc<TextLayout>,
    used: u64,
    bytes: usize,
}

/// Mutable cache state guarded by a single mutex.
#[derive(Default)]
struct Inner {
    styles: FxHashMap<StyleKey, FxHashMap<Box<str>, Entry>>,
    entries: usize,
    bytes: usize,
    clock: u64,
}

/// Retained thread-safe layout cache keyed by style, width, and content.
#[derive(Clone, Default)]
pub struct ShapedTextCache {
    inner: Arc<Mutex<Inner>>,
}

impl ShapedTextCache {
    /// Constructs a clean text shaping cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up a pre-computed layout, marking it most-recently-used.
    pub fn get(&self, params: &TextShapeParams) -> Option<Arc<TextLayout>> {
        let key = StyleKey::from_params(params);
        let mut inner = self.inner.lock().unwrap();
        inner.clock += 1;
        let clock = inner.clock;
        let entry = inner.styles.get_mut(&key)?.get_mut(params.text)?;
        entry.used = clock;
        Some(Arc::clone(&entry.layout))
    }

    /// Approximate lookup for resize drags: same style (ignoring width)
    /// and content, width within `tolerance_quanta` (1/16px units).
    ///
    /// During a live resize every frame crosses width buckets, so exact
    /// lookups miss continuously and each miss pays a full Parley shape.
    /// A nearby width reuses an already-shaped layout (wrap may differ by
    /// a subpixel boundary word — invisible in motion, exact on release
    /// when the hint expires and misses shape precisely). Best match wins;
    /// entries are NOT stored under the requested key, so the cache stays
    /// exact. Non-finite widths never fuzzy-match. Scan capped at 256
    /// style buckets.
    pub fn get_nearby(
        &self,
        params: &TextShapeParams,
        tolerance_quanta: u32,
    ) -> Option<Arc<TextLayout>> {
        let target = StyleKey::from_params(params);
        if target.max_width_bits == u32::MAX {
            return None;
        }
        let mut inner = self.inner.lock().unwrap();
        let mut best: Option<(f32, StyleKey, Arc<TextLayout>)> = None;
        let mut scanned = 0u32;
        // Stored widths are quantum counts as floats (round(v*16)), so the
        // difference is already in 1/16px units — compare against the quantum
        // tolerance directly.
        for (sk, bucket) in inner.styles.iter() {
            if scanned >= 256 {
                break;
            }
            if !sk.same_style(&target) || sk.max_width_bits == u32::MAX {
                continue;
            }
            scanned += 1;
            let d = (f32::from_bits(sk.max_width_bits)
                - f32::from_bits(target.max_width_bits))
            .abs();
            if !(d <= tolerance_quanta as f32) {
                continue;
            }
            if let Some(entry) = bucket.get(params.text) {
                let replace = match &best {
                    None => true,
                    Some((bd, _, _)) => d < *bd,
                };
                if replace {
                    best = Some((d, sk.clone(), Arc::clone(&entry.layout)));
                }
            }
        }
        let (_, key, layout) = best?;
        // Recency bump so burst reuse doesn't age the winner out.
        inner.clock += 1;
        let clock = inner.clock;
        if let Some(entry) = inner
            .styles
            .get_mut(&key)
            .and_then(|b| b.get_mut(params.text))
        {
            entry.used = clock;
        }
        Some(layout)
    }
    /// Stores a shaped layout, evicting the least-recently-used entry at capacity.
    pub fn store(&self, params: &TextShapeParams, layout: Arc<TextLayout>) {
        let key = StyleKey::from_params(params);
        let mut inner = self.inner.lock().unwrap();
        while inner.entries >= MAX_CACHE_ENTRIES || inner.bytes >= MAX_CACHE_BYTES {
            let before = inner.entries;
            inner.evict_lru();
            if inner.entries == before {
                break;
            }
        }
        inner.clock += 1;
        let used = inner.clock;
        let bytes = entry_bytes(params.text.len(), &layout);
        let bucket = inner.styles.entry(key).or_default();
        match bucket.insert(params.text.into(), Entry { layout, used, bytes }) {
            None => {
                inner.entries += 1;
                inner.bytes += bytes;
            }
            Some(old) => {
                inner.bytes = inner.bytes.saturating_sub(old.bytes) + bytes;
            }
        }
    }
}

impl Inner {
    /// Removes one entry at capacity in O(1) amortized time.
    ///
    /// Previously a global least-recently-used scan over all <=2048 entries on every
    /// insert at capacity (O(E)). The cache is a throughput memo, not a hit-rate-critical
    /// page cache: evicting an arbitrary (oldest-clock in first non-empty bucket) entry
    /// keeps insert O(1) while retaining LRU ordering within each bucket via `used`.
    fn evict_lru(&mut self) {
        // O(buckets) worst case, O(1) typical: drop the oldest entry of the first
        // non-empty bucket instead of scanning every entry globally.
        let mut victim: Option<(StyleKey, Box<str>)> = None;
        for (key, bucket) in &self.styles {
            let mut oldest: Option<(&Box<str>, u64)> = None;
            for (content, entry) in bucket.iter() {
                if oldest.is_none_or(|(_, t)| entry.used < t) {
                    oldest = Some((content, entry.used));
                }
                // Bound the scan per bucket to keep eviction cheap on huge buckets.
                if bucket.len() > 64 && oldest.is_some() {
                    break;
                }
            }
            if let Some((content, _)) = oldest {
                victim = Some((key.clone(), content.clone()));
                break;
            }
        }
        let Some((key, content)) = victim else { return };
        if let Some(bucket) = self.styles.get_mut(&key) {
            if let Some(old) = bucket.remove(&content) {
                self.entries = self.entries.saturating_sub(1);
                self.bytes = self.bytes.saturating_sub(old.bytes);
            }
            if bucket.is_empty() {
                self.styles.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{Constraints, IntrinsicSize};
    use crate::text::fonts::FontStyle;

    fn params_for(text: &str, max_width: f32) -> TextShapeParams<'_> {
        TextShapeParams {
            text,
            font: None,
            family: None,
            style: FontStyle::Normal,
            size: 16.0,
            line_height: 20.0,
            letter_spacing: 0.0,
            align: TextAlign::Start,
            wrap: TextWrap::Wrap,
            overflow: TextOverflow::Clip,
            max_lines: None,
            weight: 400,
            variations: &[],
            constraints: Constraints::loose(max_width, f32::INFINITY),
        }
    }

    fn dummy_layout() -> Arc<TextLayout> {
        Arc::new(TextLayout::new(
            Vec::new(),
            IntrinsicSize {
                width: 10.0,
                height: 20.0,
            },
        ))
    }

    /// Nearby widths reuse within tolerance during resize drags, miss
    /// outside it, and never match across styles or content.
    #[test]
    fn nearby_width_reuse() {
        let cache = ShapedTextCache::new();
        let stored = dummy_layout();
        cache.store(&params_for("hello", 400.0), Arc::clone(&stored));

        // 400.4px = 6.4 quanta away: within ±8.
        let near = cache
            .get_nearby(&params_for("hello", 400.4), 8)
            .expect("nearby width must hit");
        assert!(Arc::ptr_eq(&near, &stored));
        // Exact lookup still misses (cache stays exact, not polluted).
        assert!(cache.get(&params_for("hello", 400.4)).is_none());
        // 401.0px = 16 quanta: outside tolerance.
        assert!(cache.get_nearby(&params_for("hello", 401.0), 8).is_none());
        // Different content or style never fuzzy-matches.
        assert!(cache.get_nearby(&params_for("other", 400.4), 8).is_none());
        let mut wide = params_for("hello", 400.4);
        wide.size = 20.0;
        assert!(cache.get_nearby(&wide, 8).is_none());
    }

    /// Fractional width jitter within 1/16px must hit the same entry:
    /// this is what keeps animated/resizing text off the cold-shape path.
    #[test]
    fn fractional_widths_share_one_cache_entry() {
        let cache = ShapedTextCache::new();
        let stored = dummy_layout();
        cache.store(&params_for("hello", 400.0), Arc::clone(&stored));

        for width in [400.0, 400.01, 400.03, 399.98, 400.02] {
            let hit = cache
                .get(&params_for("hello", width))
                .expect("jittered width must hit");
            assert!(
                Arc::ptr_eq(&hit, &stored),
                "width {width} must reuse the 400.0 entry"
            );
        }
    }

    /// Widths beyond the quantum and different strings must still miss.
    #[test]
    fn distant_widths_and_strings_miss() {
        let cache = ShapedTextCache::new();
        cache.store(&params_for("hello", 400.0), dummy_layout());

        assert!(cache.get(&params_for("hello", 500.0)).is_none());
        assert!(cache.get(&params_for("other", 400.0)).is_none());
    }

    /// Non-finite and signed-zero widths collapse instead of keying apart.
    #[test]
    fn degenerate_widths_share_one_bucket() {
        assert_eq!(quantize_bits(f32::INFINITY), quantize_bits(f32::NAN));
        assert_eq!(quantize_bits(0.0), quantize_bits(-0.0));
    }
}
