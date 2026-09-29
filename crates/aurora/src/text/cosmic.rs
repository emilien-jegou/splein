// Single responsibility: Production text shaping, HarfBuzz ligatures, and wrapping via cosmic-text.

use crate::foundation::{IntrinsicSize, Point};
use crate::text::fonts::FontId;
use crate::text::glyph::{GlyphKey, ShapedGlyph, ShapedLine};
use crate::text::layout::TextLayout;
use crate::text::shaper::{TextShapeParams, TextShaper};
use cosmic_text::{
    fontdb, Attrs, Buffer, CacheKey, CacheKeyFlags, Family, FontSystem, Metrics, Shaping,
    SubpixelBin, Weight,
};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

/// Production text layout shaper wrapping cosmic-text with zero system font scanning.
#[derive(Clone)]
pub struct CosmicTextEngine {
    pub(crate) font_system: Arc<Mutex<FontSystem>>,
    pub(crate) key_map: Arc<Mutex<std::collections::HashMap<GlyphKey, CacheKey>>>,
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
        }
    }

    /// Loads TrueType font bytes directly into memory without disk scan.
    pub fn load_font(&self, bytes: &[u8]) {
        self.font_system
            .lock()
            .unwrap()
            .db_mut()
            .load_font_data(bytes.to_vec());
    }
}

impl Default for CosmicTextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextShaper for CosmicTextEngine {
    fn shape(&self, params: TextShapeParams) -> TextLayout {
        let safe_size = if params.size.is_finite() && params.size > 0.0 {
            params.size
        } else {
            14.0
        };
        let safe_lh = if params.line_height.is_finite() && params.line_height > 0.0 {
            params.line_height
        } else {
            safe_size * 1.2
        };
        let safe_spacing = if params.letter_spacing.is_finite() {
            params.letter_spacing
        } else {
            0.0
        };

        let mut fs = self.font_system.lock().unwrap();
        let metrics = Metrics::new(safe_size, safe_lh);
        let mut buffer = Buffer::new(&mut fs, metrics);

        let max_w =
            if params.constraints.max_width.is_finite() && params.constraints.max_width > 0.0 {
                Some(params.constraints.max_width)
            } else {
                None
            };
        buffer.set_size(&mut fs, max_w, None);

        let family_name: Option<String> = if let Some(FontId(id)) = params.font {
            fs.db()
                .faces()
                .nth(id as usize)
                .and_then(|f| f.families.first().map(|(name, _)| name.clone()))
        } else {
            None
        };

        let family = family_name
            .as_deref()
            .map(Family::Name)
            .unwrap_or(Family::SansSerif);
        let attrs = Attrs::new().family(family).weight(Weight(params.weight));
        buffer.set_text(&mut fs, params.text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut fs, false);

        let mut lines = Vec::new();
        let mut max_line_w = 0.0f32;
        let mut key_map = self.key_map.lock().unwrap();

        for run in buffer.layout_runs() {
            let mut glyphs = Vec::new();
            let mut line_w = run.line_w;
            for (idx, g) in run.glyphs.iter().enumerate() {
                let extra_spacing = if idx + 1 < run.glyphs.len() {
                    safe_spacing
                } else {
                    0.0
                };
                let ck = CacheKey {
                    flags: CacheKeyFlags::empty(),
                    font_id: g.font_id,
                    glyph_id: g.glyph_id,
                    font_size_bits: g.font_size.to_bits(),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                };

                let mut hasher = DefaultHasher::new();
                ck.hash(&mut hasher);
                let opaque_key = GlyphKey(hasher.finish());
                key_map.insert(opaque_key, ck);

                glyphs.push(ShapedGlyph {
                    glyph_id: g.glyph_id as u32,
                    point: Point::new(g.x + idx as f32 * safe_spacing, run.line_y),
                    advance: g.w + extra_spacing,
                    cluster: g.start,
                    cache_key: opaque_key,
                });
                line_w += extra_spacing;
            }
            max_line_w = max_line_w.max(line_w);
            lines.push(ShapedLine::new(
                glyphs, line_w, safe_lh, run.line_y, safe_size,
            ));
        }

        let total_h = (lines.len() as f32 * safe_lh).max(safe_lh);
        TextLayout::new(
            lines,
            IntrinsicSize {
                width: max_line_w,
                height: total_h,
            },
        )
    }
}
