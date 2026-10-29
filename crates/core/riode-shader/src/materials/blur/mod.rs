use crate::foundation::types::{Color, Geo, Stage};

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
    fn shader(&self) -> &'static str {
        BLUR_WGSL
    }

    fn write_block(&self, _geo: &Geo, block: &mut [f32; 64], dpr: f32) {
        block[0] = self.sigma * dpr;
    }

    fn pad(&self, _geo: &Geo) -> f32 {
        self.sigma * 3.0
    }

    fn sigma(&self) -> f32 {
        self.sigma
    }

    fn requires_backdrop(&self) -> bool {
        true
    }
}
