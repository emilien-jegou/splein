// Single responsibility: Retained per-chunk vertex cache for static content.
//
// Chunks whose content hash, placement, clips, and raster scale are unchanged
// replay previously emitted vertices instead of re-walking commands and
// re-rasterizing glyphs. Glyph-bearing entries are additionally tied to the
// atlas generation: an atlas repack rewrites UVs, so stale glyph ranges are
// dropped while rect-only ranges survive.

use aurora::foundation::{Radius, ResolvedRect, Transform};
use aurora::scene::chunk::SceneChunk;
use aurora::scene::command::{hash_commands, transform_bits};
use rustc_hash::FxHashMap;
use std::hash::{Hash, Hasher};

use crate::command::Op;
use crate::pipeline::{GlyphVertex, RectVertex, RRectVertex};

/// Cap on retained chunks; eviction drops the least-recently-used frame.
const MAX_CHUNKS: usize = 512;

fn radius_key<H: Hasher>(r: &Radius, h: &mut H) {
    match r {
        Radius::Scalar(v) => {
            0u8.hash(h);
            v.to_bits().hash(h);
        }
        Radius::Max => 1u8.hash(h),
        Radius::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => {
            2u8.hash(h);
            top_left.to_bits().hash(h);
            top_right.to_bits().hash(h);
            bottom_right.to_bits().hash(h);
            bottom_left.to_bits().hash(h);
        }
    }
}

fn rect_key<H: Hasher>(r: &ResolvedRect, h: &mut H) {
    r.x.to_bits().hash(h);
    r.y.to_bits().hash(h);
    r.width.to_bits().hash(h);
    r.height.to_bits().hash(h);
}

/// Identity of a chunk's emitted output: content + placement + clips + scale.
pub fn chunk_key(chunk: &SceneChunk, scale_q16: u16) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    hash_commands(&chunk.commands).hash(&mut h);
    transform_bits(&chunk.placement).hash(&mut h);
    chunk.abs_origin.x.to_bits().hash(&mut h);
    chunk.abs_origin.y.to_bits().hash(&mut h);
    scale_q16.hash(&mut h);
    chunk.inherited_clips.len().hash(&mut h);
    for clip in chunk.inherited_clips.iter() {
        rect_key(&clip.rect, &mut h);
        radius_key(&clip.radius, &mut h);
    }
    h.finish()
}

pub struct CachedChunk {
    pub rects: Vec<RectVertex>,
    pub glyphs: Vec<GlyphVertex>,
    pub rrects: Vec<RRectVertex>,
    /// Ops with starts relative to the range beginnings (rebase on replay).
    pub ops: Vec<Op>,
    pub atlas_gen: u64,
    pub has_glyphs: bool,
    last_frame: u64,
}

impl CachedChunk {
    pub fn new(
        rects: Vec<RectVertex>,
        glyphs: Vec<GlyphVertex>,
        rrects: Vec<RRectVertex>,
        ops: Vec<Op>,
        atlas_gen: u64,
    ) -> Self {
        let has_glyphs = !glyphs.is_empty();
        Self {
            rects,
            glyphs,
            rrects,
            ops,
            atlas_gen,
            has_glyphs,
            last_frame: 0,
        }
    }

    fn is_live(&self, atlas_gen: u64) -> bool {
        !self.has_glyphs || self.atlas_gen == atlas_gen
    }
}

/// Content-addressed chunk vertex store.
#[derive(Default)]
pub struct ChunkCache {
    map: FxHashMap<u64, CachedChunk>,
    frame: u64,
    pub hits: u64,
    pub misses: u64,
}

impl ChunkCache {
    pub fn next_frame(&mut self) {
        self.frame += 1;
    }

    pub fn get(&mut self, key: u64, atlas_gen: u64) -> Option<&CachedChunk> {
        let f = self.frame;
        let e = self.map.get_mut(&key)?;
        if !e.is_live(atlas_gen) {
            self.map.remove(&key);
            self.misses += 1;
            return None;
        }
        e.last_frame = f;
        self.hits += 1;
        Some(e)
    }

    pub fn insert(&mut self, key: u64, mut chunk: CachedChunk) {
        chunk.last_frame = self.frame;
        self.map.insert(key, chunk);
        self.misses += 1;
        if self.map.len() > MAX_CHUNKS {
            // Evict the stalest frame; a linear scan only runs on overflow.
            if let Some(victim) = self
                .map
                .iter()
                .min_by_key(|(_, c)| c.last_frame)
                .map(|(k, _)| *k)
            {
                self.map.remove(&victim);
            }
        }
    }

    /// Drops all retained chunks (e.g. on resize: baked device-px verts
    /// are stale at the new size). Atlas and glyph state are unaffected.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Drops all glyph-bearing entries (e.g. after an atlas repack —
    /// normally handled lazily via `is_live`, this is the eager path).
    pub fn clear_glyphs(&mut self) {
        self.map.retain(|_, c| !c.has_glyphs);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Rebase a cached op's vertex range onto the live buffer lengths.
    pub fn rebased(op: &Op, rect_base: u32, glyph_base: u32, rrect_base: u32) -> Op {
        match *op {
            Op::BaseQuads { start, count } => Op::BaseQuads {
                start: start + rect_base,
                count,
            },
            Op::Rects {
                start,
                count,
                scissor,
            } => Op::Rects {
                start: start + rect_base,
                count,
                scissor,
            },
            Op::Glyphs {
                start,
                count,
                scissor,
                linear,
            } => Op::Glyphs {
                start: start + glyph_base,
                count,
                scissor,
                linear,
            },
            Op::RRects {
                start,
                count,
                scissor,
            } => Op::RRects {
                start: start + rrect_base,
                count,
                scissor,
            },
        }
    }
}

/// Placement + origin as a single transform, matching the renderer's
/// `translate(abs_origin) * placement` composition.
pub fn chunk_base(chunk: &SceneChunk) -> Transform {
    Transform::from_translation(chunk.abs_origin.x, chunk.abs_origin.y)
        .multiply(&chunk.placement)
}
