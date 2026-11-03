use riode::foundation::{Geo, Stage};

pub const ROUNDED_RECT_WGSL: &str = r#"
fn stage_apply(payload: ptr<function, FragmentPayload>, uniforms: vec4<f32>) {
    let half_dim = vec2<f32>(geo.width, geo.height) * 0.5;
    let p = (*payload).pos - half_dim;
    
    // Scale radii by dpr (uniforms are logical px: [tl, tr, br, bl])
    var r = uniforms.x * geo.dpr;
    if (p.x > 0.0 && p.y < 0.0) {
        r = uniforms.y * geo.dpr;
    } else if (p.x > 0.0 && p.y > 0.0) {
        r = uniforms.z * geo.dpr;
    } else if (p.x < 0.0 && p.y > 0.0) {
        r = uniforms.w * geo.dpr;
    }
    
    r = min(r, min(half_dim.x, half_dim.y));
    let q = abs(p) - half_dim + vec2<f32>(r, r);
    (*payload).dist = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}
"#;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    pub radius: [f32; 4],
}

impl RoundedRect {
    pub fn rect() -> Self {
        Self { radius: [0.0; 4] }
    }

    pub fn rounded(r: f32) -> Self {
        Self {
            radius: [r, r, r, r],
        }
    }

    pub fn corners(tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        Self {
            radius: [tl, tr, br, bl],
        }
    }
}

impl Stage for RoundedRect {
    fn wgsl_source(&self) -> &'static str {
        ROUNDED_RECT_WGSL
    }

    fn uniform_size(&self) -> usize {
        16 // vec4<f32>
    }

    fn write_uniforms(&self, _geo: &Geo, buffer: &mut [u8]) {
        for (i, val) in self.radius.iter().enumerate() {
            buffer[i * 4..(i + 1) * 4].copy_from_slice(&val.to_ne_bytes());
        }
    }
}
