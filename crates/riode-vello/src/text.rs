// Single responsibility: High-performance native GPU vector glyph rasterization for Vello.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use vello::glyph::Glyph;
use vello::kurbo::Affine;
use vello::peniko::{Blob, Brush, Fill as VelloFillRule, Font as PenikoFont};
use vello::Scene as VelloScene;

use crate::shader::to_vello_color;
use riode::foundation::{Color, Point};
use riode::text::{TextContext, TextLayout};

/// GPU typography engine rendering shaped runs natively as vector glyph outlines.
#[derive(Default)]
pub struct VelloGlyphCache {
    font_cache: FxHashMap<usize, PenikoFont>,
}

impl VelloGlyphCache {
    /// Constructs a clean glyph cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders shaped typography runs directly into Vello compute scene as vector glyphs.
    pub fn render_text(
        &mut self,
        scene: &mut VelloScene,
        text_ctx: &TextContext,
        origin: Point,
        layout: &TextLayout,
        color: Color,
        tx: Affine,
    ) {
        if layout.lines.is_empty() {
            return;
        }

        let brush = Brush::Solid(to_vello_color(color, 1.0));
        let base_tx = tx * Affine::translate((origin.x as f64, origin.y as f64));

        for line in &layout.lines {
            let mut start = 0;
            while start < line.glyphs.len() {
                let blob = line.glyphs[start].font_blob;
                // Variable-font instance of the run head. The blob alone
                // resolves to the default instance, so runs must also split
                // on coords or every weight renders as Regular.
                let head_coords = text_ctx
                    .glyph_coords(line.glyphs[start].cache_key)
                    .unwrap_or_default();
                let mut end = start + 1;
                while end < line.glyphs.len()
                    && line.glyphs[end].font_blob == blob
                    && text_ctx
                        .glyph_coords(line.glyphs[end].cache_key)
                        .unwrap_or_default()
                        == head_coords
                {
                    end += 1;
                }

                if let Some(font_bytes) = text_ctx.font_data(blob) {
                    let ptr_key = Arc::as_ptr(&font_bytes) as usize;
                    let peniko_font = self.font_cache.entry(ptr_key).or_insert_with(|| {
                        PenikoFont::new(Blob::from(font_bytes.as_slice().to_vec()), 0)
                    });
                    let glyphs = line.glyphs[start..end].iter().map(|g| Glyph {
                        id: g.glyph_id,
                        x: g.point.x + line.align_offset,
                        y: g.point.y,
                    });

                    // F2Dot14 bits <-> i16: swash/parley coords map 1:1.
                    let coords: Vec<vello::skrifa::instance::NormalizedCoord> = head_coords
                        .iter()
                        .map(|&raw| vello::skrifa::instance::NormalizedCoord::from_bits(raw))
                        .collect();
                    scene
                        .draw_glyphs(peniko_font)
                        .font_size(line.font_size)
                        .normalized_coords(&coords)
                        .transform(base_tx)
                        .brush(&brush)
                        .draw(VelloFillRule::NonZero, glyphs);
                }
                start = end;
            }
        }
    }
}
