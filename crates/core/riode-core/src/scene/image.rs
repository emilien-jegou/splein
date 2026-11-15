// Responsibility: Backend-agnostic raster image buffer representation and binary artifact loader.

use std::path::Path;
use std::sync::Arc;
use maio_core::AssetId;

const HEADER_SIZE: usize = 64;
const MAGIC: &[u8; 4] = b"MAIO";

#[derive(Clone, Debug, PartialEq)]
pub struct ImageSource {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
}

impl ImageSource {
    #[must_use]
    pub fn from_rgba8(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        assert_eq!((width * height * 4) as usize, bytes.len(), "Image buffer size mismatch");
        Self { width, height, data: Arc::new(bytes) }
    }

    #[must_use]
    pub fn from_static_rgba8(width: u32, height: u32, bytes: &'static [u8]) -> Self {
        assert_eq!((width * height * 4) as usize, bytes.len(), "Image buffer size mismatch");
        Self { width, height, data: Arc::new(bytes.to_vec()) }
    }

    /// Reads a pre-baked binary artifact from disk.
    #[must_use]
    pub fn from_asset(id: [u8; 16], width: u32, height: u32, asset_dir: &Path) -> Self {
        let asset_id = AssetId::from_bytes(id);
        let path = asset_dir.join(format!("{}.bin", asset_id.as_hex()));

        match std::fs::read(&path) {
            Ok(bytes) if bytes.len() >= HEADER_SIZE && &bytes[0..4] == MAGIC => {
                let actual_w = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
                let actual_h = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
                let payload_len = u64::from_le_bytes(bytes[56..64].try_into().unwrap()) as usize;
                let pixels = bytes[HEADER_SIZE..HEADER_SIZE + payload_len].to_vec();
                Self::from_rgba8(actual_w, actual_h, pixels)
            }
            _ => Self::empty_placeholder(width, height),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::from_static_rgba8(1, 1, b"\0\0\0\0")
    }

    #[must_use]
    pub fn empty_placeholder(width: u32, height: u32) -> Self {
        let (w, h) = (width.max(1), height.max(1));
        Self { width: w, height: h, data: Arc::new(vec![0u8; (w * h * 4) as usize]) }
    }

    #[must_use]
    pub fn aspect_ratio(&self) -> Option<f32> {
        (self.width > 0 && self.height > 0).then(|| self.width as f32 / self.height as f32)
    }
}
