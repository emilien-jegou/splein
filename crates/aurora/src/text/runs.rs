// Single responsibility: Converting Parley layout lines into Aurora shaped lines.

#![cfg_attr(not(feature = "text-runtime"), allow(dead_code))]
use crate::foundation::Point;
use crate::text::engine::{ParleyEngine, RasterKey};
use crate::text::glyph::{GlyphKey, ShapedGlyph, ShapedLine};
#[cfg(feature = "text-runtime")]
use parley::{Layout, PositionedLayoutItem};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use std::hash::{Hash, Hasher};

/// Pending raster-key insert, collected lock-free and flushed in one batch.
#[cfg(feature = "text-runtime")]
struct PendingKey {
    key: GlyphKey,
    font: parley::FontData,
    glyph_id: u32,
    size_bits: u32,
    coords: Vec<i16>,
}

/// Converts the layout's lines into shaped lines with one entry per visual line.
///
/// Runs inside a Parley line are merged: selection, decoration, and ellipsis
/// code all treat a visual line as one glyph sequence keyed by cluster bytes.
#[cfg(feature = "text-runtime")]
pub(crate) fn extract_lines(
    engine: &ParleyEngine,
    layout: &Layout<()>,
    text: &str,
    line_height: f32,
    font_size: f32,
) -> (Vec<ShapedLine>, f32) {
    let mut lines = Vec::new();
    let mut max_line_w = 0.0f32;
    // Raster inserts are collected lock-free and flushed once at the end, so
    // the shared map is held for a single batch insert instead of the whole
    // extraction (hashing, blob resolve, glyph pushes all run unlocked).
    let mut pending: Vec<PendingKey> = Vec::new();
    // Keys already queued in this extraction: repeated glyphs (every second
    // 'e') skip the FontData clone and coords allocation, not just the insert.
    let mut queued: FxHashSet<GlyphKey> = FxHashSet::default();
    // Blob lookup per shaped font, hoisted out of the per-glyph path: runs are
    // usually single-font, so the id-map hit below runs once per run.
    let mut blob_cache: FxHashMap<BlobCacheKey, usize> = FxHashMap::default();

    for (line_idx, line) in layout.lines().enumerate() {
        // Uniform line boxes stacked from zero, matching the old cosmic
        // rhythm: selection math divides y by the line height.
        let line_top = line_idx as f32 * line_height;
        let baseline = line_top + line.metrics().baseline;

        // Cluster byte offsets per run in visual glyph order. Glyph runs split
        // runs at style boundaries, so the flattened glyph sequence of a run
        // is re-keyed here and consumed positionally below.
        let mut run_clusters: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
        for run in line.runs() {
            let mut starts = Vec::new();
            for cluster in run.visual_clusters() {
                let start = cluster.text_range().start;
                starts.extend(cluster.glyphs().map(|_| start));
            }
            run_clusters.insert(run.index() as u32, starts);
        }
        let mut consumed: FxHashMap<u32, usize> = FxHashMap::default();

        let mut glyphs = Vec::new();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let run_idx = run.index() as u32;
            // Quantized raster identity (see `raster_size_bits`); shaping and
            // layout keep full precision.
            let font_size_bits = raster_size_bits(run.font_size());
            let coords = run.normalized_coords();
            let blob = blob_for_font(engine, &mut blob_cache, run.font());
            let starts: &[usize] = run_clusters.get(&run_idx).map_or(&[], Vec::as_slice);
            let mut pos = consumed.get(&run_idx).copied().unwrap_or(0);
            for g in glyph_run.positioned_glyphs() {
                let cluster = starts.get(pos).copied().unwrap_or(0);
                pos += 1;

                // Raster identity: (font, glyph, size, variation instance).
                // Shaping is deterministic, so identical inputs share one key.
                // The blob id is stable per registered font; the face index
                // disambiguates collections packed in one binary.
                let mut hasher = FxHasher::default();
                run.font().data.id().hash(&mut hasher);
                run.font().index.hash(&mut hasher);
                g.id.hash(&mut hasher);
                font_size_bits.hash(&mut hasher);
                coords.hash(&mut hasher);
                let opaque_key = GlyphKey(hasher.finish());
                if queued.insert(opaque_key) {
                    pending.push(PendingKey {
                        key: opaque_key,
                        font: run.font().clone(),
                        glyph_id: g.id,
                        size_bits: font_size_bits,
                        coords: coords.to_vec(),
                    });
                }

                glyphs.push(ShapedGlyph {
                    glyph_id: g.id,
                    point: Point::new(g.x, line_top + g.y),
                    advance: g.advance,
                    cluster,
                    cache_key: opaque_key,
                    font_blob: blob,
                });
            }
            consumed.insert(run_idx, pos);
        }

        let width = line_content_width(&glyphs, text);
        max_line_w = max_line_w.max(width);
        lines.push(ShapedLine::new(
            glyphs,
            width,
            line_height,
            baseline,
            font_size,
        ));
    }

    // Single batch flush: insert-only, the same key always maps to the same
    // entry, and `or_insert_with` keeps the first writer's value.
    if !pending.is_empty() {
        let mut key_map = engine.key_map.lock().unwrap();
        for p in pending {
            key_map.entry(p.key).or_insert_with(|| RasterKey {
                font: p.font,
                glyph_id: p.glyph_id,
                size_bits: p.size_bits,
                coords: p.coords,
            });
        }
    }

    // Collapse phantom blank lines: Parley can emit an empty visual line
    // around explicit breaks, so one `\n` in the source paints as two line
    // boxes (a double return). Runs of blanks collapse to one, and a
    // trailing empty line without a trailing newline is dropped. Wrapped
    // lines (all non-empty) are untouched. Re-stack on kept index so
    // selection math (`y / line_height`) stays exact; per-line baseline
    // offsets are preserved via the `baselines` side table.
    {
        let ends_with_break = text.ends_with('\n');
        let mut kept: Vec<ShapedLine> = Vec::with_capacity(lines.len());
        let mut kept_off: Vec<f32> = Vec::with_capacity(lines.len());
        let mut prev_empty = false;
        for line in lines.into_iter() {
            let empty = line.glyphs.is_empty();
            // Baseline offset within its uniform box, captured before filter.
            let base_off = line_off(line_height, &line);
            if empty {
                if prev_empty {
                    continue;
                }
                prev_empty = true;
            } else {
                prev_empty = false;
            }
            kept_off.push(base_off);
            kept.push(line);
        }
        if !ends_with_break {
            while kept.last().is_some_and(|l| l.glyphs.is_empty()) {
                kept.pop();
                kept_off.pop();
            }
        }
        for (idx, (line, off)) in kept.iter_mut().zip(kept_off.iter()).enumerate() {
            let new_top = idx as f32 * line_height;
            let old_top = line.baseline - off;
            let dy = new_top - old_top;
            if dy != 0.0 {
                for g in &mut line.glyphs {
                    g.point.y += dy;
                }
            }
            line.baseline = new_top + off;
            line.height = line_height;
        }
        // Recompute max width after filtering (unchanged for wrapped text).
        max_line_w = kept.iter().fold(0.0f32, |m, l| m.max(l.width));
        lines = kept;
    }

    (lines, max_line_w)
}

/// Offset of `baseline` inside its uniform line box.
#[cfg(feature = "text-runtime")]
fn line_off(line_height: f32, line: &ShapedLine) -> f32 {
    let lh = line_height.max(1e-6);
    let off = line.baseline - (line.baseline / lh).floor() * lh;
    if off.is_finite() && off >= 0.0 && off <= line_height {
        off
    } else {
        line_height * 0.8
    }
}

/// Raster size quantum: 1/64px, as raw bits for cache identity.
///
/// Exact float sizes would mint a distinct bitmap per animation frame and flip
/// hinting on/off as sizes jitter across the cutoff. Coverage is
/// size-sensitive so the quantum is finer than the shape cache's 1/16px, but
/// 1/64px (0.016px) is far below visibility. Glyph advances and positions
/// keep full precision — only the raster identity is quantized.
#[cfg(feature = "text-runtime")]
pub(crate) fn raster_size_bits(size: f32) -> u32 {
    if !size.is_finite() || size <= 0.0 {
        return size.to_bits();
    }
    ((size * 64.0).round() / 64.0).to_bits()
}

/// Key for the per-run blob lookup: identity of the shared font resource.
#[derive(Clone, PartialEq, Eq, Hash)]
struct BlobCacheKey {
    id: u64,
    index: u32,
}

/// Resolves the shaped font to a `TextContext` blob index for the vello path.
///
/// The registration blob id map answers in O(1); the byte comparison below
/// only runs when fontique re-wrapped the blob (unknown fonts fall back to
/// blob zero, mirroring the old single-blob assumption).
#[cfg(feature = "text-runtime")]
fn blob_for_font(
    engine: &ParleyEngine,
    cache: &mut FxHashMap<BlobCacheKey, usize>,
    font: &parley::FontData,
) -> usize {
    // Blob ids are stable per registered font binary.
    let key = BlobCacheKey {
        id: font.data.id(),
        index: font.index,
    };
    if let Some(blob) = cache.get(&key) {
        return *blob;
    }
    // O(1) registration lookup first. The byte comparison fallback runs with
    // no other map held: holding `blob_ids` across the `font_blobs` lock
    // would invert the order `load_font` uses and risk deadlock.
    if let Some(blob) = engine.blob_ids.lock().unwrap().get(&key.id).copied() {
        cache.insert(key, blob);
        return blob;
    }
    let bytes: &[u8] = font.data.data();
    let blob = engine
        .font_blobs
        .lock()
        .unwrap()
        .iter()
        .position(|b| b.as_slice() == bytes)
        .unwrap_or(0);
    cache.insert(key, blob);
    blob
}

/// Content width of a line: trailing whitespace advances are trimmed so
/// centering and wrap-budget checks ignore the break opportunity's space.
#[cfg(feature = "text-runtime")]
fn line_content_width(glyphs: &[ShapedGlyph], text: &str) -> f32 {
    let mut width = 0.0f32;
    for g in glyphs {
        width = width.max(g.point.x + g.advance);
    }
    // Walk back over trailing whitespace clusters; the first non-space glyph
    // ends the content.
    let bytes = text.as_bytes();
    let mut end = width;
    for g in glyphs.iter().rev() {
        let is_ws = matches!(bytes.get(g.cluster), Some(b) if b.is_ascii_whitespace());
        if !is_ws {
            break;
        }
        end = end.min(g.point.x);
    }
    end
}
