// Single responsibility: Thread-safe memoization cache for shaped typography layouts.

use crate::text::fonts::FontId;
use crate::text::layout::TextLayout;
use crate::text::shaper::TextShapeParams;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// FIX: key now includes line_height and letter_spacing. Previously, changing
// either silently returned a stale shaped layout.
type TextCacheKey = (
    String,          // content
    Option<FontId>,  // font
    u32,             // size_bits
    u16,             // weight
    u32,             // line_height_bits
    u32,             // letter_spacing_bits
    u32,             // max_width_bits (0 = unconstrained or single-line)
);
const MAX_CACHE_ENTRIES: usize = 2048;

/// Retained thread-safe layout cache keyed by content, font, and width constraints.
#[derive(Clone, Default)]
pub struct ShapedTextCache {
    inner: Arc<Mutex<HashMap<TextCacheKey, TextLayout>>>,
}

impl ShapedTextCache {
    /// Constructs a clean text shaping cache.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::with_capacity(256))),
        }
    }

    /// Looks up a pre-computed layout for the given shaping parameters.
    pub fn get(&self, params: &TextShapeParams) -> Option<TextLayout> {
        let key = Self::make_key(params);
        self.inner.lock().unwrap().get(&key).cloned()
    }

    /// Stores a shaped layout, clearing entries if capacity threshold is exceeded.
    pub fn store(&self, params: &TextShapeParams, layout: TextLayout) {
        let key = Self::make_key(params);
        let mut map = self.inner.lock().unwrap();
        if map.len() >= MAX_CACHE_ENTRIES {
            map.clear();
        }
        map.insert(key, layout);
    }

    fn make_key(params: &TextShapeParams) -> TextCacheKey {
        let is_multiline = params.text.contains('\n');
        let max_w_bits = if is_multiline && params.constraints.max_width.is_finite() {
            params.constraints.max_width.to_bits()
        } else {
            0
        };
        (
            params.text.to_string(),
            params.font,
            params.size.to_bits(),
            params.weight,
            params.line_height.to_bits(),
            params.letter_spacing.to_bits(),
            max_w_bits,
        )
    }
}
