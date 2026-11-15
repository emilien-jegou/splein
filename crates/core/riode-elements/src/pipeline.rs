// Responsibility: Domain task definition and in-memory rasterization processor for image assets.

use image::imageops::FilterType;
use maio_compiler::{ProcessorError, TaskProcessor};
use maio_core::{AssetSource, AssetTask, TaskSpec};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const IMAGE_RASTER_KIND: &str = "image:rasterize";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRasterTask {
    pub path: PathBuf,
    pub content_hash: [u8; 16],
    pub width: u32,
    pub height: u32,
}

impl TaskSpec for ImageRasterTask {
    const KIND: &'static str = IMAGE_RASTER_KIND;

    fn source(&self) -> &AssetSource {
        // Safe stack reference constructed for the trait query
        static mut S: Option<AssetSource> = None;
        unsafe {
            S = Some(AssetSource::Path {
                path: self.path.clone(),
                content_hash: self.content_hash,
            });
            S.as_ref().unwrap()
        }
    }
}

#[derive(Default)]
pub struct ImageRasterProcessor;

impl TaskProcessor for ImageRasterProcessor {
    fn process(&self, task: &AssetTask, source_bytes: &[u8]) -> Result<Vec<u8>, ProcessorError> {
        let spec: ImageRasterTask = task.to_spec().map_err(|e| ProcessorError::Format(e.to_string()))?;
        let img = image::load_from_memory(source_bytes)
            .map_err(|e| ProcessorError::Execution(e.to_string()))?;

        let filter = if cfg!(debug_assertions) { FilterType::Triangle } else { FilterType::Lanczos3 };
        let resized = if spec.width == img.width() && spec.height == img.height() {
            img
        } else {
            img.resize_exact(spec.width.max(1), spec.height.max(1), filter)
        };

        let mut rgba = resized.into_rgba8();
        for pixel in rgba.chunks_exact_mut(4) {
            let a = pixel[3] as u32;
            pixel[0] = ((pixel[0] as u32 * a) / 255) as u8;
            pixel[1] = ((pixel[1] as u32 * a) / 255) as u8;
            pixel[2] = ((pixel[2] as u32 * a) / 255) as u8;
        }

        Ok(rgba.into_raw())
    }
}
