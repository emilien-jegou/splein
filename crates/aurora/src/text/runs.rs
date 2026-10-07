// Single responsibility: Converting cosmic-text layout runs into Aurora shaped lines.

use crate::foundation::Point;
use crate::text::cosmic::CosmicTextEngine;
use crate::text::glyph::{GlyphKey, ShapedGlyph, ShapedLine};
use cosmic_text::{fontdb, CacheKey, CacheKeyFlags, SubpixelBin};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// Converts the buffer's layout runs into shaped lines, applying inter-glyph tracking.
pub(crate) fn extract_lines(
    engine: &CosmicTextEngine,
    buffer: &cosmic_text::Buffer,
    spacing: f32,
    line_height: f32,
    font_size: f32,
) -> (Vec<ShapedLine>, f32) {
    let mut lines = Vec::new();
    let mut max_line_w = 0.0f32;
    let mut key_map = engine.key_map.lock().unwrap();
    let id_to_blob = engine.id_to_blob.lock().unwrap();

    for run in buffer.layout_runs() {
        let mut glyphs = Vec::new();
        let mut line_w = run.line_w;
        let count = run.glyphs.len();
        // Runs are usually single-font: remember the last blob lookup
        // instead of hashing the id map per glyph.
        let mut last_blob: Option<(fontdb::ID, usize)> = None;
        for (idx, g) in run.glyphs.iter().enumerate() {
            let extra_spacing = if idx + 1 < count { spacing } else { 0.0 };
            let ck = CacheKey {
                flags: CacheKeyFlags::empty(),
                font_id: g.font_id,
                glyph_id: g.glyph_id,
                font_size_bits: g.font_size.to_bits(),
                x_bin: SubpixelBin::Zero,
                y_bin: SubpixelBin::Zero,
            };

            let mut hasher = FxHasher::default();
            ck.hash(&mut hasher);
            let opaque_key = GlyphKey(hasher.finish());
            // Insert-only: the same key always maps to the same entry.
            key_map.entry(opaque_key).or_insert(ck);

            let blob = match last_blob {
                Some((id, blob)) if id == g.font_id => blob,
                _ => {
                    let blob = id_to_blob.get(&g.font_id).copied().unwrap_or(0);
                    last_blob = Some((g.font_id, blob));
                    blob
                }
            };
            glyphs.push(ShapedGlyph {
                glyph_id: g.glyph_id as u32,
                point: Point::new(g.x + idx as f32 * spacing, run.line_y),
                advance: g.w + extra_spacing,
                cluster: g.start,
                cache_key: opaque_key,
                font_blob: blob,
            });
            line_w += extra_spacing;
        }
        max_line_w = max_line_w.max(line_w);
        lines.push(ShapedLine::new(
            glyphs,
            line_w,
            line_height,
            run.line_y,
            font_size,
        ));
    }

    (lines, max_line_w)
}
