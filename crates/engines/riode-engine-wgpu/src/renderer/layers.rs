// Single responsibility: Offscreen filter layer pool, targets, and nesting stack.

use riode::foundation::ResolvedRect;

/// One filter-layer offscreen slot: surface-sized Rgba8Unorm, same
/// format/coords as the persistent target so scene passes, scissors and
/// verts work unchanged inside the layer; texture-bound so a nested
/// layer can snapshot this layer as its backdrop.
pub fn make_filter_target(
    device: &wgpu::Device,
    w: u32,
    h: u32,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST, // <-- add COPY_DST
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// One open offscreen layer at draw time: its pool slot index.
pub struct OpenLayer {
    pub slot: usize,
}

/// Manages the pool of surface-sized intermediate layer textures.
pub struct LayerPool {
    pub pool: Vec<(wgpu::Texture, wgpu::TextureView)>,
    pub stack: Vec<OpenLayer>,
    pub stale_root_layers: Vec<ResolvedRect>,
}

impl LayerPool {
    pub fn new() -> Self {
        Self {
            pool: Vec::new(),
            stack: Vec::new(),
            stale_root_layers: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.pool.clear();
        self.stack.clear();
        self.stale_root_layers.clear();
    }

    pub fn ensure_slot(&mut self, device: &wgpu::Device, slot: usize, sw: u32, sh: u32) {
        while self.pool.len() <= slot {
            let i = self.pool.len();
            self.pool.push(make_filter_target(
                device,
                sw,
                sh,
                &format!("riode-wgpu-layer-{i}"),
            ));
        }
    }

    pub fn parent_view<'a>(&'a self, persistent: &'a wgpu::TextureView) -> &'a wgpu::TextureView {
        match self.stack.last() {
            Some(l) => &self.pool[l.slot].1,
            None => persistent,
        }
    }
}
