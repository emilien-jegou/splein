// Single responsibility: Offscreen readback staging and synchronous texture map.

use riode::render::BackendError;
use crate::context::WgpuContext;

pub struct ReadbackBuffer {
    cached: Option<(u32, u32, wgpu::Buffer)>,
}

impl ReadbackBuffer {
    pub fn new() -> Self {
        Self { cached: None }
    }

    pub fn readback_rgba8(&mut self, gpu: &WgpuContext) -> Result<Vec<u8>, BackendError> {
        let (w, h) = (gpu.config.width.max(1), gpu.config.height.max(1));
        let pitch = ((w * 4 + 255) / 256 * 256) as usize;
        let size = (pitch * h as usize) as u64;

        let buf = match &self.cached {
            Some((bw, bh, b)) if *bw == w && *bh == h => b,
            _ => {
                let b = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("riode-wgpu-readback"),
                    size: size.max(256),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                self.cached = Some((w, h, b));
                &self.cached.as_ref().unwrap().2
            }
        };

        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("riode-wgpu-readback"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &gpu.persistent,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch as u32),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        gpu.queue.submit(Some(encoder.finish()));

        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        gpu.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| BackendError::raster("readback map channel closed"))?
            .map_err(|e| BackendError::raster(format!("readback map failed: {e:?}")))?;

        let out = {
            let mapped = slice.get_mapped_range();
            let mut tight = vec![0u8; (w * h * 4) as usize];
            for y in 0..h as usize {
                tight[y * w as usize * 4..(y + 1) * w as usize * 4]
                    .copy_from_slice(&mapped[y * pitch..y * pitch + w as usize * 4]);
            }
            tight
        };
        buf.unmap();
        Ok(out)
    }
}
