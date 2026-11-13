// Responsibility: Manages retained offscreen filter texture targets, composite bindings, and nesting stack.

use riode::foundation::ResolvedRect;

pub struct LayerSlot {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub composite_bg: wgpu::BindGroup,
}

pub fn make_filter_slot(
    device: &wgpu::Device,
    filter_layout: &wgpu::BindGroupLayout,
    w: u32,
    h: u32,
    label: &str,
) -> LayerSlot {
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
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let composite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("{label}-composite-bg")),
        layout: filter_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });

    LayerSlot {
        texture,
        view,
        composite_bg,
    }
}

pub struct OpenLayer {
    pub slot: usize,
}

pub struct LayerPool {
    pub pool: Vec<LayerSlot>,
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

    pub fn ensure_slot(
        &mut self,
        device: &wgpu::Device,
        filter_layout: &wgpu::BindGroupLayout,
        slot: usize,
        sw: u32,
        sh: u32,
    ) {
        while self.pool.len() <= slot {
            let i = self.pool.len();
            self.pool.push(make_filter_slot(
                device,
                filter_layout,
                sw,
                sh,
                &format!("riode-wgpu-layer-{i}"),
            ));
        }
    }

    pub fn parent_view<'a>(&'a self, persistent: &'a wgpu::TextureView) -> &'a wgpu::TextureView {
        match self.stack.last() {
            Some(l) => &self.pool[l.slot].view,
            None => persistent,
        }
    }
}
