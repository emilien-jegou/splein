// Single responsibility: Font database storage, lifetime management, and shaping dispatcher.

use crate::foundation::Constraints;
use crate::text::cache::ShapedTextCache;
use crate::text::config::TextConfig;
use crate::text::cosmic::CosmicTextEngine;
use crate::text::glyph::GlyphKey;
use crate::text::layout::TextLayout;
use crate::text::shaper::{TextShapeParams, TextShaper};
use cosmic_text::SwashCache;
use std::sync::{Arc, Mutex};

/// Rasterized pixel buffer and metrics for an individual glyph.
pub struct GlyphBitmap<'a> {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

/// Retained typography context providing font loading, layout shaping, and glyph rasterization.
#[derive(Clone)]
pub struct TextContext {
    pub(crate) engine: CosmicTextEngine,
    swash_cache: Arc<Mutex<SwashCache>>,
    cache: ShapedTextCache,
    pub(crate) font_blobs: Arc<Mutex<Vec<Arc<Vec<u8>>>>>,
}

impl TextContext {
    /// Constructs a clean text context with an empty font database.
    pub fn new() -> Self {
        Self {
            engine: CosmicTextEngine::new(),
            swash_cache: Arc::new(Mutex::new(SwashCache::new())),
            cache: ShapedTextCache::new(),
            font_blobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Loads TrueType/OpenType font bytes into context and retains binary handle for GPU engines.
    pub fn load_font(&self, bytes: &[u8]) {
        let index = {
            let mut blobs = self.font_blobs.lock().unwrap();
            let index = blobs.len();
            blobs.push(Arc::new(bytes.to_vec()));
            index
        };
        self.engine.load_font(bytes, index);
    }

    /// Loads a font file from disk into the database for lazy system registration.
    pub fn load_font_file(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let bytes = std::fs::read(path)?;
        self.load_font(&bytes);
        Ok(())
    }

    /// Accesses retained raw font binary data by registration index.
    pub fn font_data(&self, index: usize) -> Option<Arc<Vec<u8>>> {
        self.font_blobs.lock().unwrap().get(index).cloned()
    }

    /// Shapes a text string into multi-line glyph runs under given constraints.
    pub fn shape(&self, params: TextShapeParams) -> Arc<TextLayout> {
        if let Some(cached) = self.cache.get(&params) {
            return cached;
        }
        let layout = Arc::new(self.engine.shape(params));
        self.cache.store(&params, Arc::clone(&layout));
        layout
    }

    /// Shapes text directly from configuration specification under layout constraints.
    pub fn shape_config(&self, config: &TextConfig, constraints: Constraints) -> Arc<TextLayout> {
        self.shape(TextShapeParams {
            text: &config.content,
            font: config.font_id,
            family: config.family.as_deref(),
            style: config.style,
            size: config.size,
            line_height: config.line_height.resolve(config.size),
            letter_spacing: config.letter_spacing,
            align: config.align,
            overflow: config.overflow,
            max_lines: config.max_lines,
            weight: config.weight,
            constraints,
        })
    }

    /// Rasterizes a glyph by opaque key and invokes consumer closure with bitmap data.
    pub fn raster_glyph<R>(&self, key: GlyphKey, f: impl FnOnce(Option<GlyphBitmap>) -> R) -> R {
        let ck = self.engine.key_map.lock().unwrap().get(&key).copied();
        let Some(cache_key) = ck else {
            return f(None);
        };

        let mut fs = self.engine.font_system.lock().unwrap();
        let mut sc = self.swash_cache.lock().unwrap();
        let image = sc.get_image(&mut fs, cache_key);

        match image {
            Some(ref img) => f(Some(GlyphBitmap {
                left: img.placement.left,
                top: img.placement.top,
                width: img.placement.width,
                height: img.placement.height,
                data: &img.data,
            })),
            None => f(None),
        }
    }
}

impl Default for TextContext {
    fn default() -> Self {
        Self::new()
    }
}
