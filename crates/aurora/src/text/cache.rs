// Single responsibility: Thread-safe memoization cache for shaped typography layouts.

use crate::text::align::TextAlign;
use crate::text::fonts::{FontId, FontStyle};
use crate::text::layout::TextLayout;
use crate::text::overflow::TextOverflow;
use crate::text::shaper::TextShapeParams;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

const MAX_CACHE_ENTRIES: usize = 2048;

/// Style and width identity shared by every string shaped the same way.
#[derive(Clone, PartialEq, Eq, Hash)]
struct StyleKey {
    font: Option<FontId>,
    family: Option<String>,
    style: FontStyle,
    size_bits: u32,
    weight: u16,
    line_height_bits: u32,
    letter_spacing_bits: u32,
    align: TextAlign,
    overflow: TextOverflow,
    max_lines: Option<u32>,
    max_width_bits: u32,
}

impl StyleKey {
    /// Builds a style identity from shaping parameters, ignoring the content string.
    fn from_params(params: &TextShapeParams) -> Self {
        Self {
            font: params.font,
            family: params.family.map(str::to_string),
            style: params.style,
            size_bits: params.size.to_bits(),
            weight: params.weight,
            line_height_bits: params.line_height.to_bits(),
            letter_spacing_bits: params.letter_spacing.to_bits(),
            align: params.align,
            overflow: params.overflow,
            max_lines: params.max_lines,
            max_width_bits: if params.constraints.max_width.is_finite() {
                params.constraints.max_width.to_bits()
            } else {
                0
            },
        }
    }
}

/// Cached layout plus its last-use clock for LRU eviction.
struct Entry {
    layout: Arc<TextLayout>,
    used: u64,
}

/// Mutable cache state guarded by a single mutex.
#[derive(Default)]
struct Inner {
    styles: FxHashMap<StyleKey, FxHashMap<Box<str>, Entry>>,
    entries: usize,
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

    /// Stores a shaped layout, evicting the least-recently-used entry at capacity.
    pub fn store(&self, params: &TextShapeParams, layout: Arc<TextLayout>) {
        let key = StyleKey::from_params(params);
        let mut inner = self.inner.lock().unwrap();
        if inner.entries >= MAX_CACHE_ENTRIES {
            inner.evict_lru();
        }
        inner.clock += 1;
        let used = inner.clock;
        let bucket = inner.styles.entry(key).or_default();
        let inserted = bucket.insert(params.text.into(), Entry { layout, used });
        if inserted.is_none() {
            inner.entries += 1;
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
            if bucket.remove(&content).is_some() {
                self.entries = self.entries.saturating_sub(1);
            }
            if bucket.is_empty() {
                self.styles.remove(&key);
            }
        }
    }
}
