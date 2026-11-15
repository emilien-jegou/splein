// Responsibility: In-memory image decoding, Lanczos3/Triangle resampling, and RGBA8 pixel packing.

use image::imageops::FilterType;
use maio_compiler::{ProcessedOutput, ProcessorError, TaskProcessor};
use maio_core::AssetTask;
use super::task::ImageTask;

#[derive(Default, Clone)]
pub struct ImageProcessor;

impl TaskProcessor for ImageProcessor {
    fn process(&self, task: &AssetTask, source_bytes: &[u8]) -> Result<ProcessedOutput, ProcessorError> {
        let spec: ImageTask = task.to_spec()
            .map_err(|e| ProcessorError::Format(format!("failed to decode ImageTask: {e}")))?;

        let img = image::load_from_memory(source_bytes)
            .map_err(|e| ProcessorError::Execution(format!("decode failed: {e}")))?;

        let filter = if cfg!(debug_assertions) { FilterType::Triangle } else { FilterType::Lanczos3 };
        let pw = spec.width.max(1);
        let ph = spec.height.max(1);

        let resized = if pw == img.width() && ph == img.height() {
            img
        } else {
            img.resize_exact(pw, ph, filter)
        };

        let mut rgba = resized.into_rgba8();
        for pixel in rgba.chunks_exact_mut(4) {
            let a = pixel[3] as u32;
            pixel[0] = ((pixel[0] as u32 * a) / 255) as u8;
            pixel[1] = ((pixel[1] as u32 * a) / 255) as u8;
            pixel[2] = ((pixel[2] as u32 * a) / 255) as u8;
        }

        Ok(ProcessedOutput {
            width: pw,
            height: ph,
            payload: rgba.into_raw(),
        })
    }
}
