// Single responsibility: Dynamic vertex, glyph, image, and uniform ring buffers.

use std::cell::Cell;
use crate::pipeline::{GlyphVertex, ImageVertex, RectVertex};

fn make_buffer(
    device: &wgpu::Device,
    label: &str,
    bytes: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.max(256),
        usage,
        mapped_at_creation: false,
    })
}

/// Dynamic GPU buffers for batch geometry and dynamic material uniform streams.
pub struct StagingBuffers {
    pub ortho_buf: wgpu::Buffer,
    pub ortho_bg: wgpu::BindGroup,
    pub vert_buf: wgpu::Buffer,
    pub vert_cap: u64,
    pub glyph_buf: wgpu::Buffer,
    pub glyph_cap: u64,
    pub image_buf: wgpu::Buffer,
    pub image_cap: u64,
    // Dynamic uniform streaming buffers for material execution.
    pub material_geo_buf: wgpu::Buffer,
    pub material_geo_cap: u64,
    pub material_geo_cursor: Cell<u32>,
    pub material_uniform_buf: wgpu::Buffer,
    pub material_uniform_cap: u64,
    pub material_uniform_cursor: Cell<u32>,
}

impl StagingBuffers {
    pub fn new(
        device: &wgpu::Device,
        ortho_layout: &wgpu::BindGroupLayout,
        w: u32,
        h: u32,
    ) -> Self {
        let (ortho_buf, ortho_bg) = Self::create_ortho(device, ortho_layout, w, h);
        let vert_cap = 4096u64;
        let glyph_cap = 4096u64;
        let image_cap = 512u64;
        let material_geo_cap = 65536u64;
        let material_uniform_cap = 65536u64;

        let vert_buf = make_buffer(
            device,
            "riode-wgpu-verts",
            vert_cap * std::mem::size_of::<RectVertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let glyph_buf = make_buffer(
            device,
            "riode-wgpu-glyphs",
            glyph_cap * std::mem::size_of::<GlyphVertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let image_buf = make_buffer(
            device,
            "riode-wgpu-images",
            image_cap * std::mem::size_of::<ImageVertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let material_geo_buf = make_buffer(
            device,
            "riode-wgpu-material-geo-ring",
            material_geo_cap,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let material_uniform_buf = make_buffer(
            device,
            "riode-wgpu-material-uniform-ring",
            material_uniform_cap,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        Self {
            ortho_buf,
            ortho_bg,
            vert_buf,
            vert_cap,
            glyph_buf,
            glyph_cap,
            image_buf,
            image_cap,
            material_geo_buf,
            material_geo_cap,
            material_geo_cursor: Cell::new(0),
            material_uniform_buf,
            material_uniform_cap,
            material_uniform_cursor: Cell::new(0),
        }
    }

    pub fn create_ortho(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        w: u32,
        h: u32,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        use wgpu::util::DeviceExt;
        let data = [w.max(1) as f32, h.max(1) as f32, 0.0, 0.0];
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riode-wgpu-ortho"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riode-wgpu-ortho-bg"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        (buf, bg)
    }

    /// Resets dynamic uniform ring offsets at the start of each frame.
    pub fn reset_material_cursors(&self) {
        self.material_geo_cursor.set(0);
        self.material_uniform_cursor.set(0);
    }

    /// Allocates an offset into the dynamic Geo uniform buffer (aligned to 256 bytes).
    pub fn alloc_geo_offset(&self) -> u32 {
        let offset = self.material_geo_cursor.get();
        self.material_geo_cursor.set(offset + 256);
        offset
    }

    /// Allocates an offset into the dynamic Material uniform buffer (aligned to 256 bytes).
    pub fn alloc_uniform_offset(&self, size: usize) -> u32 {
        let aligned = ((size as u32 + 255) / 256) * 256;
        let offset = self.material_uniform_cursor.get();
        self.material_uniform_cursor.set(offset + aligned);
        offset
    }

    pub fn ensure_rect_cap(&mut self, device: &wgpu::Device, need: u64) {
        if need <= self.vert_cap { return; }
        let mut cap = self.vert_cap.max(1);
        while cap < need { cap *= 2; }
        self.vert_buf = make_buffer(
            device,
            "riode-wgpu-verts",
            cap * std::mem::size_of::<RectVertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        self.vert_cap = cap;
    }

    pub fn ensure_glyph_cap(&mut self, device: &wgpu::Device, need: u64) {
        if need <= self.glyph_cap { return; }
        let mut cap = self.glyph_cap.max(1);
        while cap < need { cap *= 2; }
        self.glyph_buf = make_buffer(
            device,
            "riode-wgpu-glyphs",
            cap * std::mem::size_of::<GlyphVertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        self.glyph_cap = cap;
    }

    pub fn ensure_image_cap(&mut self, device: &wgpu::Device, need: u64) {
        if need <= self.image_cap { return; }
        let mut cap = self.image_cap.max(1);
        while cap < need { cap *= 2; }
        self.image_buf = make_buffer(
            device,
            "riode-wgpu-images",
            cap * std::mem::size_of::<ImageVertex>() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        self.image_cap = cap;
    }
}
