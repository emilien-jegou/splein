use crate::foundation::types::{Geo, Stage};

pub const BLUR_WGSL: &str = include_str!("blur.wgsl");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blur {
    pub sigma: f32,
}

impl Blur {
    pub fn new(sigma: f32) -> Self {
        Self { sigma }
    }
}

impl Stage for Blur {
    fn wgsl_source(&self) -> &'static str {
        BLUR_WGSL
    }

    fn uniform_size(&self) -> usize {
        16 // one f32 + 12 bytes WGSL uniform padding
    }

    fn write_uniforms(&self, _geo: &Geo, buffer: &mut [u8]) {
        buffer[..4].copy_from_slice(&self.sigma.to_ne_bytes());
    }

    fn requires_backdrop(&self) -> bool {
        true
    }
}
