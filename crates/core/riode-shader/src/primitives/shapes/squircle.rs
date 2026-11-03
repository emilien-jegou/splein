use riode::foundation::{Geo, Stage};

pub const SQUIRCLE_WGSL: &str = r#"
fn stage_apply(payload: ptr<function, FragmentPayload>, uniforms: vec4<f32>) {
    let half_dim = vec2<f32>(geo.width, geo.height) * 0.5;
    let p = (*payload).pos - half_dim;

    // Radius in physical device pixels
    var r = uniforms.x * geo.dpr;
    r = min(r, min(half_dim.x, half_dim.y));

    // Curvature exponent (e.g. 4.0 for standard iOS-style squircle, 2.0 for standard circle)
    let n = max(uniforms.y, 1.0);
    let inv_n = 1.0 / n;

    let q = abs(p) - half_dim + vec2<f32>(r, r);
    let outside = max(q, vec2<f32>(0.0));

    // Parametric p-norm for smooth corner curvature
    let k = pow(pow(outside.x, n) + pow(outside.y, n), inv_n);
    let inside = min(max(q.x, q.y), 0.0);

    (*payload).dist = k + inside - r;
}
"#;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Squircle {
    /// Logical corner radius
    pub radius: f32,
    /// Curvature exponent ($p$-norm): 4.0 is standard squircle, 2.0 is circular
    pub exponent: f32,
}

impl Squircle {
    pub const DEFAULT_EXPONENT: f32 = 4.0;

    /// Standard squircle with the given radius and default $n = 4.0$ curvature.
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            exponent: Self::DEFAULT_EXPONENT,
        }
    }

    /// Squircle with explicit radius and curvature exponent $n$.
    pub fn with_exponent(radius: f32, exponent: f32) -> Self {
        Self { radius, exponent }
    }
}

impl Stage for Squircle {
    fn wgsl_source(&self) -> &'static str {
        SQUIRCLE_WGSL
    }

    fn uniform_size(&self) -> usize {
        16 // vec4<f32>: [radius, exponent, 0.0, 0.0]
    }

    fn write_uniforms(&self, _geo: &Geo, buffer: &mut [u8]) {
        let uniforms = [self.radius, self.exponent, 0.0f32, 0.0f32];
        for (i, val) in uniforms.iter().enumerate() {
            buffer[i * 4..(i + 1) * 4].copy_from_slice(&val.to_ne_bytes());
        }
    }
}
