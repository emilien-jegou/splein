// Single responsibility: GPU glyph atlas with guttered shelf packing + LRU repack.

use aurora::text::glyph::GlyphKey;
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

/// Shelf-packed coverage atlas over R8Unorm.
///
/// Eviction is repack-based LRU: on overflow, cells unused in the last
/// STALE_FRAMES are dropped and survivors re-uploaded into a fresh shelf.
/// Steady-state UI glyph sets fit without ever repacking.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    map: FxHashMap<AtlasSlot, Cell>,
    cursor: Cursor,
    frame: u64,
    gen: u64,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("aurora-wgpu-glyph-atlas"),
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

    fn upload_cell(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: &[u8],
    ) {
        // Guttered upload: replicated border around the bitmap so linear
        // border taps read our own edge, never a neighbor.
        let (cw, ch) = ((w + 2 * GUTTER) as usize, (h + 2 * GUTTER) as usize);
        let mut padded = vec![0u8; cw * ch];
        for row in 0..ch {
            let sy = ((row as u32).saturating_sub(GUTTER)).min(h.saturating_sub(1));
            for col in 0..cw {
                let sx = ((col as u32).saturating_sub(GUTTER)).min(w.saturating_sub(1));
                padded[row * cw + col] = data[(sy * w + sx) as usize];
            }
        }
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &padded,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(cw as u32),
                rows_per_image: Some(ch as u32),
            },
            wgpu::Extent3d {
                width: cw as u32,
                height: ch as u32,
                depth_or_array_layers: 1,
            },
        );
    }

    fn entry_for(x: u32, y: u32, w: u32, h: u32) -> AtlasEntry {
        let s = ATLAS_SIZE as f32;
        // UVs address the inner bitmap exactly edge-to-edge: the 2px
        // replicated gutter already guarantees linear border taps read our
        // own edge texels, so no half-texel inset is needed. (The inset
        // shrank every glyph by a texel — a permanent minification blur.)
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

    /// Drops stale cells and re-uploads survivors into a fresh shelf.
    fn repack(&mut self, queue: &wgpu::Queue) {
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
        for (k, data, w, h) in live {
            if let Some((x, y, cur)) = self.probe(w, h) {
                Self::upload_cell(queue, &self.texture, x, y, w, h, &data);
                self.cursor = cur;
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
        tracing::debug!(
            dropped,
            live = self.map.len(),
            "aurora-wgpu glyph atlas repacked"
        );
        self.gen += 1;
    }

    /// Uploads a coverage bitmap, returning its atlas entry.
    /// Returns None for empty bitmaps or glyphs larger than the atlas.
    /// `scale_q16` is the quantized raster scale (distinct slots per scale).
    pub fn insert(
        &mut self,
        queue: &wgpu::Queue,
        key: GlyphKey,
        phase: u8,
        scale_q16: u16,
        w: u32,
        h: u32,
        data: &[u8],
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
        Self::upload_cell(queue, &self.texture, x, y, w, h, data);
        self.cursor = cur;
        let entry = Self::entry_for(x, y, w, h);
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
}
