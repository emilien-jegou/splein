// Responsibility: Strongly typed asset task specification for raster images.

use maio_core::TaskSpec;
use serde::{Deserialize, Serialize};

pub const IMAGE_KIND: &str = "image:rasterize";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageTask {
    pub width: u32,
    pub height: u32,
}

impl TaskSpec for ImageTask {
    const KIND: &'static str = IMAGE_KIND;
}
