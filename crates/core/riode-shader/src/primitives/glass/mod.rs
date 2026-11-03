pub const MODULAR_GLASS_WGSL: &str = include_str!("modular_glass.wgsl");

// Concrete implementations for modular glass stages.
// These are placed here as the shader crate is the source of truth for WGSL.
// The Stage/Material traits are defined in riode; riode may re-export or use these.

pub mod stage_mask {
    pub const REFRACT: u32 = 1;
    pub const FROST: u32 = 2;
    pub const BEER: u32 = 4;
    pub const FILL: u32 = 8;
}

pub const K_MODULAR_FLOATS: usize = 64;

pub const W_STAGEMASK: usize = 0;
pub const W_CORNERS: usize = 4;
pub const W_REFRACT_A: usize = 8;
pub const W_REFRACT_B: usize = 12;
pub const W_ORIGIN: usize = 16;
pub const W_FROST: usize = 20;
pub const W_BEER: usize = 28;
pub const W_FILL: usize = 32;

// Placeholder types moved from riode - glass logic moves to shader crate
#[derive(Clone, Debug)]
pub struct Glass;

#[derive(Clone, Debug)]
pub struct GlassStyle;

#[derive(Clone, Debug)]
pub struct GlassLayer;

#[derive(Clone, Debug)]
pub struct GlassParams;

#[derive(Clone, Debug)]
pub struct GlassUniforms;

#[derive(Clone, Debug)]
pub struct ToneCurve {
    pub knots: [f32; 9],
    pub small_knots: [f32; 9],
    pub small_lo: f32,
    pub small_hi: f32,
    pub luma_mix: f32,
    pub lift: f32,
    pub lift_knee: f32,
    pub lift_size_ref: f32,
}

#[derive(Clone, Debug)]
pub struct GlassPressGeometry {
    pub translation: (f32, f32),
    pub scale_x: f32,
    pub scale_y: f32,
}

#[derive(Clone, Debug)]
pub struct GlassPress;

impl GlassPress {
    pub fn is_active(&self) -> bool { false }
    pub fn geometry(&self, _motion: &()) -> GlassPressGeometry {
        GlassPressGeometry { translation: (0.0, 0.0), scale_x: 1.0, scale_y: 1.0 }
    }
    pub fn geometry_for(&self, _motion: &(), _touch: (f32,f32)) -> GlassPressGeometry {
        GlassPressGeometry { translation: (0.0, 0.0), scale_x: 1.0, scale_y: 1.0 }
    }
    pub fn advance(&mut self) {}
    pub fn up(&mut self, _motion: &()) {}
    pub fn retarget_down(&mut self, _touch: (f32,f32), _w: f32, _h: f32, _motion: &()) {}
    pub fn move_to(&mut self, _touch: (f32,f32)) {}
    pub fn last_touch(&self) -> (f32,f32) { (0.0,0.0) }
    pub fn down(touch: (f32,f32), w: f32, h: f32, motion: &()) -> Self { GlassPress }
}
