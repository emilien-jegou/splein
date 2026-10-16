// Single responsibility: GPU texture cache for raster images (premultiplied
// RGBA8, sRGB-encoded, CPU box-filtered mip chain).
//
// Convention: `ImageSource` bytes are *premultiplied* RGBA8 (matching the
// tiny-skia engine's `Pixmap::from_vec` and vello's internal expectation),
// sRGB-encoded per channel. Textures are plain `Rgba8Unorm` — NOT the sRGB
// format: hardware sRGB decode would apply the transfer curve to the
// premultiplied channels, and decode(rgb*a) != decode(rgb)*a, which darkens
// and grays semi-transparent edges. The shader un-premultiplies, decodes,
// and re-premultiplies (straight-alpha-in-encoded-space -> linear
// premultiplied), then the pipeline blends with PREMULTIPLIED_ALPHA
// into the sRGB target (hardware encodes on store).
//
// Keyed by allocation identity (`Arc::as_ptr` on the pixel buffer), the
// same identity `hash_commands` uses — chunk-cache keys and texture keys
// agree for free. Eviction is LRU by a frame clock; entries held by a
// `CachedChunk` (via `Arc<ImageGpu>`) outlive map eviction, so a replay
// never dangles (re-emission after eviction simply re-uploads).

use riode::scene::image::ImageSource;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Count cap, matching the tiny-skia engine.
const MAX_CACHED_IMAGES: usize = 64;
/// Byte budget for retained textures (source bytes, excluding mip overhead).
const MAX_IMAGE_BYTES: usize = 32 << 20;

/// A retained GPU texture plus its view and prebuilt bind group.
pub struct ImageGpu {
    /// Kept alive so the wgpu resource outlives map eviction while a
    /// cached chunk still replays draws against the bind group.
    _texture: wgpu::Texture,
    /// Source pixels kept alive so the allocation-identity cache key can
    /// never be reused by a different image while this entry is live
    /// (same guard the tiny-skia engine applies).
    _source: Arc<Vec<u8>>,
    pub view: wgpu::TextureView,
    pub bind: wgpu::BindGroup,
    /// Source dimensions (byte budget accounting after eviction).
    pub w: u32,
    pub h: u32,
}

pub struct ImageCache {
    map: FxHashMap<usize, Arc<ImageGpu>>,
    /// Last-use clock per key (stalest evicted, never arbitrary).
    used: FxHashMap<usize, u64>,
    clock: u64,
    bytes: usize,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl ImageCache {
    pub fn new(device: &wgpu::Device) -> Self {
        // Structurally identical to the glyph layout (texture + filtering
        // sampler); wgpu compares bind group layouts structurally, so the
        // image pipeline's group 1 accepts these bind groups.
        let layout = crate::pipeline::glyph_bind_layout(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("riode-wgpu-image-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            map: FxHashMap::default(),
            used: FxHashMap::default(),
            clock: 0,
            bytes: 0,
            layout,
            sampler,
        }
    }

    /// Advances the LRU clock. Call once per frame.
    pub fn next_frame(&mut self) {
        self.clock += 1;
    }

    /// Returns the GPU texture for an image source, uploading (with a full
    /// CPU mip chain) on first use. `None` only for empty images.
    pub fn get_or_insert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &ImageSource,
    ) -> Option<Arc<ImageGpu>> {
        if image.width == 0 || image.height == 0 {
            return None;
        }
        let key = Arc::as_ptr(&image.data) as usize;
        if let Some(hit) = self.map.get(&key) {
            let hit = hit.clone();
            self.used.insert(key, self.clock);
            return Some(hit);
        }
        while self.map.len() >= MAX_CACHED_IMAGES || self.bytes >= MAX_IMAGE_BYTES {
            let before = self.map.len();
            self.evict_oldest();
            if self.map.len() == before {
                break;
            }
        }
        let gpu = Arc::new(upload(device, queue, &self.layout, &self.sampler, image));
        self.bytes += image.data.len();
        self.map.insert(key, gpu.clone());
        self.used.insert(key, self.clock);
        Some(gpu)
    }

    fn evict_oldest(&mut self) {
        let Some(victim) = self
            .used
            .iter()
            .min_by_key(|(_, t)| **t)
            .map(|(k, _)| *k)
        else {
            return;
        };
        if let Some(old) = self.map.remove(&victim) {
            self.bytes = self.bytes.saturating_sub(old.w as usize * old.h as usize * 4);
        }
        self.used.remove(&victim);
    }
}

/// Creates the texture with a full mip chain, uploads every level, and
/// builds the bind group. Mips are a CPU box filter over premultiplied
/// bytes (averaging premultiplied channels is the correct filter order).
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &ImageSource,
) -> ImageGpu {
    let (w, h) = (image.width, image.height);
    let mip_count = (w.max(h) as f32).log2().floor() as u32 + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("riode-wgpu-image"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level: Vec<u8> = image.data.as_ref().clone();
    let (mut lw, mut lh) = (w, h);
    for mip in 0..mip_count {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(lw * 4),
                rows_per_image: Some(lh),
            },
            wgpu::Extent3d {
                width: lw,
                height: lh,
                depth_or_array_layers: 1,
            },
        );
        if mip + 1 >= mip_count {
            break;
        }
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        level = box_downsample(&level, lw, lh, nw, nh);
        lw = nw;
        lh = nh;
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("riode-wgpu-image-bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    ImageGpu {
        _texture: texture,
        _source: Arc::clone(&image.data),
        view,
        bind,
        w,
        h,
    }
}

/// 2x2 box filter over premultiplied RGBA8; odd source dims average the
/// clamped edge pixel (a max(1, ...) destination always has coverage).
fn box_downsample(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for dy in 0..dh {
        for dx in 0..dw {
            let sx0 = (dx * sw / dw).min(sw - 1);
            let sy0 = (dy * sh / dh).min(sh - 1);
            let sx1 = (((dx + 1) * sw / dw).max(sx0 + 1)).min(sw);
            let sy1 = (((dy + 1) * sh / dh).max(sy0 + 1)).min(sh);
            let n = (sx1 - sx0) * (sy1 - sy0);
            let mut acc = [0u32; 4];
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = ((sy * sw + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += src[i + c] as u32;
                    }
                }
            }
            let o = ((dy * dw + dx) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / n) as u8;
            }
        }
    }
    out
}
