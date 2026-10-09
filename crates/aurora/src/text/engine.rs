// Single responsibility: Parley shaper construction, font loading, and shaping entry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use parley::fontique::{Blob, Collection, CollectionOptions};
use parley::FontContext;
use rustc_hash::FxHashMap;

use crate::text::glyph::GlyphKey;
use crate::text::layout::TextLayout;
use crate::text::overflow::{TextOverflow, truncate_to_lines};
use crate::text::shape::shape_text;
use crate::text::shaper::{TextShapeParams, TextShaper};

/// Raster identity for one shaped glyph behind an opaque [`GlyphKey`].
///
/// The same key always maps to the same entry: shaping is deterministic for
/// identical inputs, so the map is insert-only and never invalidated.
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
#[derive(Clone)]
pub struct ParleyEngine {
    pub(crate) font_ctx: Arc<Mutex<FontContext>>,
    pub(crate) key_map: Arc<Mutex<HashMap<GlyphKey, RasterKey>>>,
    pub(crate) blob_families: Arc<Mutex<Vec<Vec<String>>>>,
    /// Registration blob id → blob index: O(1) font resolution without memcmp.
    pub(crate) blob_ids: Arc<Mutex<FxHashMap<u64, usize>>>,
    /// Family resolution hoisted out of the per-string shaping path.
    pub(crate) resolved_styles: Arc<Mutex<FxHashMap<ResolvedStyleKey, Option<String>>>>,
    /// Raw font binaries by registration index, shared with `TextContext::font_data`.
    pub(crate) font_blobs: Arc<Mutex<Vec<Arc<Vec<u8>>>>>,
}

impl ParleyEngine {
    /// Constructs a clean shaper with an empty font database for 0.00ms boot time.
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
        // One Blob instance per load: its id is recorded so shaped fonts
        // resolve to this blob in O(1) without byte comparison.
        let blob = Blob::from(bytes.to_vec());
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

impl Default for ParleyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextShaper for ParleyEngine {
    fn shape(&self, params: TextShapeParams) -> TextLayout {
        let mut fcx = self.font_ctx.lock().unwrap();
        self.shape_with(&mut fcx, params)
    }
}

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
