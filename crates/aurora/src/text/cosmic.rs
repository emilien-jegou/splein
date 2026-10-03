// Single responsibility: Cosmic-text shaper construction, font loading, and shaping entry.

use std::sync::{Arc, Mutex};

use cosmic_text::{fontdb, CacheKey, FontSystem};

use crate::text::buffer_pool::BufferPool;
use crate::text::glyph::GlyphKey;
use crate::text::layout::TextLayout;
use crate::text::overflow::{truncate_to_lines, TextOverflow};
use crate::text::shape::shape_text;
use crate::text::shaper::{TextShapeParams, TextShaper};

/// Production text layout shaper wrapping cosmic-text with zero system font scanning.
#[derive(Clone)]
pub struct CosmicTextEngine {
    pub(crate) font_system: Arc<Mutex<FontSystem>>,
    pub(crate) key_map: Arc<Mutex<std::collections::HashMap<GlyphKey, CacheKey>>>,
    pub(crate) id_to_blob: Arc<Mutex<std::collections::HashMap<fontdb::ID, usize>>>,
    pub(crate) buffers: Arc<Mutex<BufferPool>>,
}

impl CosmicTextEngine {
    /// Constructs a clean shaper with empty font database for 0.00ms boot time.
    pub fn new() -> Self {
        let db = fontdb::Database::new();
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
        let font_system = FontSystem::new_with_locale_and_db(locale, db);
        Self {
            font_system: Arc::new(Mutex::new(font_system)),
            key_map: Arc::new(Mutex::new(std::collections::HashMap::with_capacity(2048))),
            id_to_blob: Arc::new(Mutex::new(std::collections::HashMap::new())),
            buffers: Arc::new(Mutex::new(BufferPool::default())),
        }
    }

    /// Loads TrueType font bytes and records the face-to-blob index mapping.
    pub fn load_font(&self, bytes: &[u8], blob_index: usize) {
        let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes.to_vec());
        let ids = self
            .font_system
            .lock()
            .unwrap()
            .db_mut()
            .load_font_source(fontdb::Source::Binary(data));
        let mut map = self.id_to_blob.lock().unwrap();
        for id in ids {
            map.insert(id, blob_index);
        }
    }
}

impl Default for CosmicTextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextShaper for CosmicTextEngine {
    fn shape(&self, params: TextShapeParams) -> TextLayout {
        let layout = shape_text(self, params.text, &params);
        if let (TextOverflow::Ellipsis, Some(max_lines)) = (params.overflow, params.max_lines) {
            let budget = max_lines.max(1) as usize;
            if layout.lines.len() > budget {
                return truncate_to_lines(&params, layout, budget, |text| {
                    shape_text(self, text, &params)
                });
            }
        }
        layout
    }
}
