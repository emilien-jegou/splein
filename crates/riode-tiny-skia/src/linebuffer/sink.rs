// Single responsibility: Row destinations for the scanline backend.

/// Destination for rasterized scanlines, from top (`y = 0`) down.
///
/// Decision: push-model (backend calls `push_line` per dirty row) rather
/// than pull, so SPI/I2C displays can stream rows without buffering a frame.
/// `data` is `width × 4` premultiplied RGBA bytes, top-down row `y`.
pub trait LineSink {
    /// Accepts one rasterized scanline.
    fn push_line(&mut self, y: u32, data: &[u8]);
    /// Completes the frame (latch, refresh, vsync — transport-specific).
    fn flush(&mut self) {}
}

/// In-memory framebuffer sink: reassembles rows for testing and for
/// memory-mapped displays. Not the point of the backend (which is *not*
/// holding a frame), but the obvious correctness oracle: byte-identical to
/// a full-frame render of the same scene and damage.
pub struct FramebufferSink {
    width: u32,
    height: u32,
    buffer: Vec<u8>,
    flushed: usize,
}

impl FramebufferSink {
    /// Allocates a `width × height` RGBA framebuffer, zeroed (transparent).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            buffer: vec![0u8; (width.max(1) * height.max(1) * 4) as usize],
            flushed: 0,
        }
    }

    /// Read access to the reassembled pixels.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Count of `flush` calls (frame counter for tests).
    pub fn flushed(&self) -> usize {
        self.flushed
    }
}

impl LineSink for FramebufferSink {
    fn push_line(&mut self, y: u32, data: &[u8]) {
        if y >= self.height {
            return;
        }
        let stride = self.width as usize * 4;
        let off = y as usize * stride;
        let len = data.len().min(stride).min(self.buffer.len() - off);
        self.buffer[off..off + len].copy_from_slice(&data[..len]);
    }

    fn flush(&mut self) {
        self.flushed += 1;
    }
}
