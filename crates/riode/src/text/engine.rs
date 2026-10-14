// Single responsibility: Parley shaper construction, font loading, and shaping entry.

#![cfg_attr(not(feature = "text-runtime"), allow(dead_code))]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(feature = "text-runtime")]
use parley::fontique::{Blob, Collection, CollectionOptions};
#[cfg(feature = "text-runtime")]
use parley::FontContext;
use rustc_hash::FxHashMap;

#[cfg(feature = "text-runtime")]
use crate::text::glyph::GlyphKey;
use crate::text::layout::TextLayout;
#[cfg(feature = "text-runtime")]
use crate::text::overflow::{truncate_to_lines, TextOverflow};
#[cfg(feature = "text-runtime")]
use crate::text::shape::shape_text;
#[cfg(feature = "text-runtime")]
use crate::text::shaper::{TextShapeParams, TextShaper};

/// Raster identity for one shaped glyph behind an opaque [`GlyphKey`].
///
/// The same key always maps to the same entry: shaping is deterministic for
/// identical inputs, so the map is insert-only and never invalidated.
#[cfg(feature = "text-runtime")]
#[derive(Clone)]
pub(crate) struct RasterKey {
    /// Shareable font binary the glyph was shaped from.
    pub font: parley::FontData,
    /// Shaped glyph identifier within the font.
    pub glyph_id: u32,
    /// Raster size in logical pixels, as raw bits.
    pub size_bits: u32,
    /// Resolved normalized variation coordinates (variable-font instance).
    pub coords: Vec<i16>,
}

/// Style inputs that determine family resolution: same key, same faces.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ResolvedStyleKey {
    pub font: Option<u32>,
    pub family: Option<String>,
}

/// Production text layout shaper wrapping Parley with zero system font scanning.
#[cfg(feature = "text-runtime")]
#[derive(Clone)]
pub struct ParleyEngine {
    pub(crate) font_ctx: Arc<Mutex<FontContext>>,
    /// Glyph raster identity side table. Deliberately insert-only: raster
    /// paths (`raster_glyph_exact`, `raster_inner`) skip glyphs whose key is
    /// absent, and shaped-layout cache hits never re-populate, so evicting
    /// here would drop glyphs until content reshapes. Entries are small
    /// (~1 Arc + 8 B + coords vec, typically <100 B); cardinality is bounded
    /// in practice by distinct glyphs ever shaped, watched by the 64k
    /// tripwire in `glyph_coords`. Do NOT add eviction without also
    /// invalidating the layouts that reference the keys.
    pub(crate) key_map: Arc<Mutex<HashMap<GlyphKey, RasterKey>>>,
    pub(crate) blob_families: Arc<Mutex<Vec<Vec<String>>>>,
    /// Registration blob id → blob index: O(1) font resolution without memcmp.
    pub(crate) blob_ids: Arc<Mutex<FxHashMap<u64, usize>>>,
    /// Family resolution hoisted out of the per-string shaping path.
    pub(crate) resolved_styles: Arc<Mutex<FxHashMap<ResolvedStyleKey, Option<String>>>>,
    /// Raw font binaries by registration index, shared with `TextContext::font_data`.
    pub(crate) font_blobs: Arc<Mutex<Vec<Arc<Vec<u8>>>>>,
}

#[cfg(feature = "text-runtime")]
impl ParleyEngine {
    /// Constructs a clean shaper with an empty font database for 0.00ms boot time.
    #[cfg(feature = "text-runtime")]
    pub fn new() -> Self {
        let collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        Self {
            font_ctx: Arc::new(Mutex::new(FontContext {
                collection,
                source_cache: Default::default(),
            })),
            key_map: Arc::new(Mutex::new(HashMap::with_capacity(2048))),
            blob_families: Arc::new(Mutex::new(Vec::new())),
            blob_ids: Arc::new(Mutex::new(FxHashMap::default())),
            resolved_styles: Arc::new(Mutex::new(FxHashMap::default())),
            font_blobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Loads TrueType/OpenType font bytes and records the blob's family names.
    pub fn load_font(&self, bytes: &[u8], blob_index: usize) {
        self.load_font_blob(Arc::new(bytes.to_vec()), blob_index);
    }

    /// Loads a font from a shared byte buffer, retaining only that buffer in
    /// the Parley collection. The same `Arc` can be shared by `TextContext`
    /// so the font is not copied twice per load.
    #[cfg(feature = "text-runtime")]
    pub fn load_font_blob(&self, bytes: Arc<Vec<u8>>, blob_index: usize) {
        let blob = Blob::new(bytes);
        let blob_id = blob.id();
        let families = {
            let mut fcx = self.font_ctx.lock().unwrap();
            let infos = fcx.collection.register_fonts(blob, None);
            let mut names = Vec::new();
            for (family_id, _) in &infos {
                if let Some(name) = fcx.collection.family_name(*family_id) {
                    names.push(name.to_string());
                }
            }
            names
        };
        let mut map = self.blob_families.lock().unwrap();
        while map.len() <= blob_index {
            map.push(Vec::new());
        }
        map[blob_index] = families;
        self.blob_ids.lock().unwrap().insert(blob_id, blob_index);
    }

    /// Resolves the requested family name, preferring an explicit family, then
    /// the `FontId` blob's first family, then the first registered family.
    ///
    /// Resolution depends only on style inputs (not on the string), so it is
    /// cached per style: hundreds of labels in one style scan the collection
    /// once instead of once per string.
    ///
    /// Takes the font collection by mutable reference: callers pass the
    /// context they already hold (shared guard or worker-local clone) so this
    /// never nests a collection lock inside itself — `std::Mutex` is not
    /// reentrant and that nesting self-deadlocks.
    pub(crate) fn resolve_family(
        &self,
        fcx: &mut FontContext,
        font: Option<crate::text::fonts::FontId>,
        family: Option<&str>,
    ) -> Option<String> {
        let style_key = ResolvedStyleKey {
            font: font.map(|f| f.0),
            family: family.map(str::to_string),
        };
        if let Some(hit) = self.resolved_styles.lock().unwrap().get(&style_key) {
            return hit.clone();
        }
        let resolved = Self::resolve_family_uncached(self, fcx, font, family);
        // Styles in flight are bounded (fonts x families actually used); skip
        // caching past the cap rather than thrashing it.
        let mut cache = self.resolved_styles.lock().unwrap();
        if cache.len() < 256 {
            cache.insert(style_key, resolved.clone());
        } else {
            // Tripwire: >256 distinct font/family styles means the style
            // identity is varying per string (e.g. generated family names) —
            // every miss pays a full collection scan. Warn once, keep working.
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!(
                    "resolved_styles past 256 entries: style identity varying per string, audit family/font inputs (further warnings silent)"
                );
            }
        }
        resolved
    }

    /// Uncached family resolution against the given font collection.
    fn resolve_family_uncached(
        &self,
        fcx: &mut FontContext,
        font: Option<crate::text::fonts::FontId>,
        family: Option<&str>,
    ) -> Option<String> {
        if let Some(name) = family {
            if fcx.collection.family_id(name).is_some() {
                return Some(name.to_string());
            }
        }
        if let Some(crate::text::fonts::FontId(id)) = font {
            if let Some(names) = self.blob_families.lock().unwrap().get(id as usize) {
                if let Some(first) = names.first() {
                    return Some(first.clone());
                }
            }
        }
        let first: Vec<String> = fcx
            .collection
            .family_names()
            .map(|name| name.to_string())
            .collect();
        first.into_iter().next()
    }
}

#[cfg(feature = "text-runtime")]
impl Default for ParleyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "text-runtime")]
impl TextShaper for ParleyEngine {
    fn shape(&self, params: TextShapeParams) -> TextLayout {
        let mut fcx = self.font_ctx.lock().unwrap();
        self.shape_with(&mut fcx, params)
    }
}

#[cfg(feature = "text-runtime")]
impl ParleyEngine {
    /// Shapes against an explicitly provided font context.
    ///
    /// Single-shape callers pass the shared collection under its lock (see
    /// `TextShaper::shape`); batch workers pass a thread-local clone so
    /// shaping parallelizes instead of convoying on one mutex.
    pub(crate) fn shape_with(&self, fcx: &mut FontContext, params: TextShapeParams) -> TextLayout {
        let layout = shape_text(self, fcx, params.text, &params);
        if let (TextOverflow::Ellipsis, Some(max_lines)) = (params.overflow, params.max_lines) {
            let budget = max_lines.max(1) as usize;
            if layout.lines.len() > budget {
                return truncate_to_lines(&params, layout, budget, |text| {
                    shape_text(self, fcx, text, &params)
                });
            }
        }
        layout
    }
}

// ── Stub engine when `text-runtime` is off ──────────────────────────────
// Same public shape as the Parley engine (fields, constructors, shaper
// trait) so layout, caches, and backends compile unchanged. Shaping yields
// empty layouts; glyph rasterization yields nothing (see context.rs stubs).

#[cfg(not(feature = "text-runtime"))]
#[derive(Clone, Debug, Default)]
pub(crate) struct StubFontData;

#[cfg(not(feature = "text-runtime"))]
#[derive(Clone)]
pub(crate) struct RasterKey {
    pub font: StubFontData,
    pub glyph_id: u32,
    pub size_bits: u32,
    pub coords: Vec<i16>,
}

/// Stub shaper: font database accepts loads (indexes retained for
/// `font_data`) but shaping always yields an empty layout.
#[cfg(not(feature = "text-runtime"))]
#[derive(Clone)]
pub struct ParleyEngine {
    pub(crate) font_ctx: Arc<Mutex<()>>,
    pub(crate) key_map: Arc<Mutex<HashMap<crate::text::glyph::GlyphKey, RasterKey>>>,
    pub(crate) blob_families: Arc<Mutex<Vec<Vec<String>>>>,
    pub(crate) blob_ids: Arc<Mutex<FxHashMap<u64, usize>>>,
    pub(crate) resolved_styles: Arc<Mutex<FxHashMap<ResolvedStyleKey, Option<String>>>>,
    pub(crate) font_blobs: Arc<Mutex<Vec<Arc<Vec<u8>>>>>,
}

#[cfg(not(feature = "text-runtime"))]
impl ParleyEngine {
    pub fn new() -> Self {
        Self {
            font_ctx: Arc::new(Mutex::new(())),
            key_map: Arc::new(Mutex::new(HashMap::new())),
            blob_families: Arc::new(Mutex::new(Vec::new())),
            blob_ids: Arc::new(Mutex::new(FxHashMap::default())),
            resolved_styles: Arc::new(Mutex::new(FxHashMap::default())),
            font_blobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn load_font(&self, bytes: &[u8], blob_index: usize) {
        drop(bytes);
        self.load_font_blob(Arc::new(Vec::new()), blob_index);
    }

    /// Stub counterpart: retain the buffer slot without Parley collection state.
    pub fn load_font_blob(&self, bytes: Arc<Vec<u8>>, blob_index: usize) {
        let mut map = self.blob_families.lock().unwrap();
        while map.len() <= blob_index {
            map.push(Vec::new());
        }
        map[blob_index] = Vec::new();
        let id = {
            use std::hash::{Hash, Hasher};
            let mut h = rustc_hash::FxHasher::default();
            bytes.len().hash(&mut h);
            bytes.get(..64).hash(&mut h);
            h.finish()
        };
        self.blob_ids.lock().unwrap().insert(id, blob_index);
        self.font_blobs.lock().unwrap().push(bytes);
    }

    pub(crate) fn resolve_family(
        &self,
        _fcx: &mut (),
        _font: Option<crate::text::fonts::FontId>,
        _family: Option<&str>,
    ) -> Option<String> {
        None
    }

    pub(crate) fn shape_with(
        &self,
        _fcx: &mut (),
        _params: crate::text::shaper::TextShapeParams,
    ) -> TextLayout {
        TextLayout::new(Vec::new(), crate::foundation::IntrinsicSize::default())
    }
}

#[cfg(not(feature = "text-runtime"))]
impl Default for ParleyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(feature = "text-runtime"))]
impl crate::text::shaper::TextShaper for ParleyEngine {
    fn shape(&self, _params: crate::text::shaper::TextShapeParams) -> TextLayout {
        TextLayout::new(Vec::new(), crate::foundation::IntrinsicSize::default())
    }
}
