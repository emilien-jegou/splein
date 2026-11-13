// Single responsibility: GPU glyph atlas with plain alpha coverage (R8, gpui-style).
// Exact coverage bitmap from the analytic rasterizer -- no distance transform approximation.
// 4x less memory than MSDF (1 byte/px vs 4), enabling more subpixel variants.

use riode::text::glyph::GlyphKey;
use riode::text::TextContext;
use rustc_hash::FxHashMap;

/// UV rect in atlas (0..1), inset to the inner bitmap (gutter excluded).
#[derive(Copy, Clone)]
pub struct AtlasEntry {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub w: u32,
    pub h: u32,
}

const ATLAS_SIZE: u32 = 2048;
/// Transparent gutter around every cell so linear taps at borders read
/// replicated edge texels instead of neighbor glyphs.
const GUTTER: u32 = 2;
/// Entries unused for this many frames are dropped on repack.
const STALE_FRAMES: u64 = 120;
struct Cell {
    entry: AtlasEntry,
    /// Retained coverage bytes (without gutter) for repack re-upload.
    /// 1 channel (R), so 1 byte per pixel.
    data: Vec<u8>,
    last_used: u64,
}

#[derive(Copy, Clone)]
struct Cursor {
    x: u32,
    y: u32,
    row_h: u32,
}

/// Atlas slot key: glyph key + subpixel phase + quantized raster scale.
/// Scaled coverage lives in distinct slots so 1x and 2x never share texels.
pub type AtlasSlot = (u64, u8, u16);

/// One dirty atlas cell waiting for the frame's single GPU upload.
///
/// `x`/`y` is the cell origin in the atlas (gutter included); `data` is
/// the guttered coverage in `cw × ch` layout, built once at insert.
struct PendingUpload {
    x: u32,
    y: u32,
    cw: u32,
    ch: u32,
    data: Vec<u8>,
}

/// Shelf-packed coverage atlas over R8Unorm (1 channel, gpui-style).
///
/// Stores the exact alpha coverage bitmap from the analytic rasterizer.
/// No distance transform -- the raw per-pixel coverage (0-255) is stored directly.
/// Shader simply multiplies vertex alpha by the sampled coverage value.
///
/// Uploads are batched: `insert` only records the cell (position + retained
/// coverage); every dirty cell accumulated during the frame reaches the GPU
/// in ONE `write_texture` when the renderer calls `flush` before the render
/// pass. No per-glyph driver submissions, no per-glyph padded buffers beyond
/// the retained data the repack already needs.
///
/// Eviction is repack-based LRU: on overflow, cells unused in the last
/// STALE_FRAMES are dropped and survivors re-uploaded into a fresh shelf —
/// also as a single `write_texture` over the repacked bounding box.
/// Steady-state UI glyph sets fit without ever repacking.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    map: FxHashMap<AtlasSlot, Cell>,
    cursor: Cursor,
    /// Cells recorded this frame, waiting for [`GlyphAtlas::flush`].
    pending: Vec<PendingUpload>,
    /// Scratch staging image, reused across flushes/repacks.
    staging: Vec<u8>,
    frame: u64,
    gen: u64,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, _text_ctx: &TextContext) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("riode-wgpu-coverage-glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            map: FxHashMap::default(),
            cursor: Cursor {
                x: 0,
                y: 0,
                row_h: 0,
            },
            pending: Vec::new(),
            staging: Vec::new(),
            frame: 0,
            gen: 0,
        }
    }

    /// Atlas content generation: bumped on every repack. Cached vertex
    /// ranges holding UVs are only replayable at the generation they were
    /// emitted against.
    pub fn generation(&self) -> u64 {
        self.gen
    }

    /// Advances the LRU clock. Call once per frame.
    pub fn next_frame(&mut self) {
        self.frame += 1;
    }

    pub fn get(&mut self, key: GlyphKey, phase: u8, scale_q16: u16) -> Option<AtlasEntry> {
        let f = self.frame;
        self.map.get_mut(&(key.0, phase, scale_q16)).map(|c| {
            c.last_used = f;
            c.entry
        })
    }

    /// Non-mutating probe: where would a w×h bitmap go?
    fn probe(&self, w: u32, h: u32) -> Option<(u32, u32, Cursor)> {
        let (cw, ch) = (w + 2 * GUTTER, h + 2 * GUTTER);
        if cw > ATLAS_SIZE || ch > ATLAS_SIZE {
            return None;
        }
        let mut cur = self.cursor;
        if cur.x + cw > ATLAS_SIZE {
            cur.y += cur.row_h;
            cur.x = 0;
            cur.row_h = 0;
        }
        if cur.y + ch > ATLAS_SIZE {
            return None;
        }
        let pos = (cur.x, cur.y);
        cur.x += cw;
        cur.row_h = cur.row_h.max(ch);
        Some((pos.0, pos.1, cur))
    }

    /// Pads raw `w × h` coverage into a guttered `(w+2G) × (h+2G)` cell,
    /// replicating edge texels outward (the old per-cell `upload_cell` body).
    fn gutter(w: u32, h: u32, data: &[u8]) -> Vec<u8> {
        // Coverage is 1 byte per pixel (R8Unorm)
        let (cw, ch) = ((w + 2 * GUTTER) as usize, (h + 2 * GUTTER) as usize);
        let mut padded = vec![0u8; cw * ch];
        for row in 0..ch {
            let sy = ((row as u32).saturating_sub(GUTTER)).min(h.saturating_sub(1));
            for col in 0..cw {
                let sx = ((col as u32).saturating_sub(GUTTER)).min(w.saturating_sub(1));
                let src_idx = (sy * w + sx) as usize;
                let dst_idx = row * cw + col;
                if src_idx < data.len() {
                    padded[dst_idx] = data[src_idx];
                }
            }
        }
        padded
    }

    /// Blits every upload into one CPU staging image and issues exactly ONE
    /// `queue.write_texture` covering the batch's bounding box.
    fn upload_batch(&mut self, queue: &wgpu::Queue, uploads: &[PendingUpload]) {
        if uploads.is_empty() {
            return;
        }
        let (mut x0, mut y0) = (u32::MAX, u32::MAX);
        let (mut x1, mut y1) = (0u32, 0u32);
        for u in uploads {
            x0 = x0.min(u.x);
            y0 = y0.min(u.y);
            x1 = x1.max(u.x + u.cw);
            y1 = y1.max(u.y + u.ch);
        }
        let (bw, bh) = (x1 - x0, y1 - y0);
        self.staging.clear();
        self.staging.resize((bw * bh) as usize, 0);
        for u in uploads {
            for row in 0..u.ch {
                let src = (row * u.cw) as usize;
                let dst = ((u.y - y0 + row) as usize) * bw as usize + (u.x - x0) as usize;
                self.staging[dst..dst + u.cw as usize]
                    .copy_from_slice(&u.data[src..src + u.cw as usize]);
            }
        }
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: x0, y: y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &self.staging,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bw),
                rows_per_image: Some(bh),
            },
            wgpu::Extent3d {
                width: bw,
                height: bh,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Uploads everything accumulated this frame: one driver submission for
    /// the whole glyph batch. Called by the renderer right before the render
    /// pass; also called internally before a repack (see [`Self::repack`]).
    pub fn flush(&mut self, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
            return;
        }
        let uploads = std::mem::take(&mut self.pending);
        self.upload_batch(queue, &uploads);
    }

    fn entry_for(x: u32, y: u32, w: u32, h: u32) -> AtlasEntry {
        let s = ATLAS_SIZE as f32;
        let (ix0, iy0) = (x + GUTTER, y + GUTTER);
        AtlasEntry {
            u0: ix0 as f32 / s,
            v0: iy0 as f32 / s,
            u1: (ix0 + w) as f32 / s,
            v1: (iy0 + h) as f32 / s,
            w,
            h,
        }
    }

    /// Drops stale cells and re-uploads survivors into a fresh shelf, batched
    /// into a single `write_texture` over the repacked bounding box.
    fn repack(&mut self, queue: &wgpu::Queue) {
        // In-flight batched uploads first: their positions are relative to
        // the current shelf cursor and must reach the GPU before the repack
        // re-places every surviving cell.
        self.flush(queue);
        let now = self.frame;
        let live: Vec<(AtlasSlot, Vec<u8>, u32, u32)> = self
            .map
            .iter()
            .filter(|(_, c)| now.saturating_sub(c.last_used) < STALE_FRAMES)
            .map(|(k, c)| (*k, c.data.clone(), c.entry.w, c.entry.h))
            .collect();
        let dropped = self.map.len().saturating_sub(live.len());
        self.map.clear();
        self.cursor = Cursor {
            x: 0,
            y: 0,
            row_h: 0,
        };
        let mut placed: Vec<PendingUpload> = Vec::with_capacity(live.len());
        for (k, data, w, h) in live {
            if let Some((x, y, cur)) = self.probe(w, h) {
                self.cursor = cur;
                placed.push(PendingUpload {
                    x,
                    y,
                    cw: w + 2 * GUTTER,
                    ch: h + 2 * GUTTER,
                    data: Self::gutter(w, h, &data),
                });
                self.map.insert(
                    k,
                    Cell {
                        entry: Self::entry_for(x, y, w, h),
                        data,
                        last_used: now,
                    },
                );
            }
        }
        self.upload_batch(queue, &placed);
        tracing::debug!(dropped, live = self.map.len(), "atlas repack");
        self.gen += 1;
    }

    /// Records a coverage bitmap into the atlas, returning its atlas entry.
    /// The GPU upload is deferred: the cell joins the frame's pending batch
    /// (see [`GlyphAtlas::flush`]). Returns None for empty bitmaps or glyphs
    /// larger than the atlas.
    /// `scale_q16` is the quantized raster scale (distinct slots per scale).
    pub fn insert(
        &mut self,
        queue: &wgpu::Queue,
        key: GlyphKey,
        phase: u8,
        scale_q16: u16,
        w: u32,
        h: u32,
        data: &[u8], // Coverage data: 1 byte per pixel
    ) -> Option<AtlasEntry> {
        if w == 0 || h == 0 {
            return None;
        }
        if let Some(e) = self.get(key, phase, scale_q16) {
            return Some(e);
        }
        let (x, y, cur) = match self.probe(w, h) {
            Some(p) => p,
            None => {
                self.repack(queue);
                self.probe(w, h)?
            }
        };
        self.cursor = cur;
        let entry = Self::entry_for(x, y, w, h);
        self.pending.push(PendingUpload {
            x,
            y,
            cw: w + 2 * GUTTER,
            ch: h + 2 * GUTTER,
            data: Self::gutter(w, h, data),
        });
        self.map.insert(
            (key.0, phase, scale_q16),
            Cell {
                entry,
                data: data.to_vec(),
                last_used: self.frame,
            },
        );
        Some(entry)
    }

    /// Insert glyph from coverage bitmap (analytic rasterizer output).
    /// Stores coverage directly -- no distance transform needed.
    pub fn insert_from_coverage(
        &mut self,
        queue: &wgpu::Queue,
        key: GlyphKey,
        phase: u8,
        scale_q16: u16,
        w: u32,
        h: u32,
        coverage: &[u8],
    ) -> Option<AtlasEntry> {
        self.insert(queue, key, phase, scale_q16, w, h, coverage)
    }
}
