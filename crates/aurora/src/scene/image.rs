// Single responsibility: Backend-agnostic raster image buffer representation.

use std::sync::Arc;

/// Retained pixel buffer for raster image display list rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageSource {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
}

impl ImageSource {
    /// Creates a new image source from raw RGBA8 pixel bytes.
    pub fn from_rgba8(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        assert_eq!((width * height * 4) as usize, bytes.len(), "Image buffer size mismatch");
        Self { width, height, data: Arc::new(bytes) }
    }
}
