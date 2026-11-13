// Single responsibility: Stroke/border shader stage conforming to the Section 3 RFC contract.

use riode::foundation::{Color, Geo, Material, Overlay, Stage};
use riode::reactive::prop::{IntoProp, Prop};

pub const STROKE_WGSL: &str = r#"
struct StrokeUniforms {
    color: vec4<f32>,
    width: f32,
}

fn stage_apply(payload: ptr<function, FragmentPayload>, uniforms: StrokeUniforms) {
    let half_w = uniforms.width * geo.dpr * 0.5;

    // Annulus / border distance: distance to the shape boundary (|d| - half_width)
    let d = abs((*payload).dist) - half_w;
    let coverage = clamp(0.5 - d, 0.0, 1.0);

    let src = uniforms.color;

    // Premultiplied alpha blend over current payload color
    let out_a = src.a * coverage + (*payload).color.a * (1.0 - src.a * coverage);
    let out_rgb = src.rgb * src.a * coverage + (*payload).color.rgb * (1.0 - src.a * coverage);
    (*payload).color = vec4<f32>(out_rgb, out_a);
}
"#;

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
}

impl Stroke {
    pub fn new(width: f32, color: impl Into<Color>) -> Self {
        Self {
            width: width.into(),
            color: color.into(),
        }
    }
}

impl Stage for Stroke {
    fn wgsl_source(&self) -> &'static str {
        STROKE_WGSL
    }

    fn uniform_size(&self) -> usize {
        // WGSL 16-byte alignment: vec4<f32> (16 bytes) + f32 + 12 bytes pad = 32 bytes
        32
    }

    fn write_uniforms(&self, _geo: &Geo, buffer: &mut [u8]) {
        // [0..16]: color (RGBA f32)
        let c = [self.color.r, self.color.g, self.color.b, self.color.a];
        for (i, val) in c.iter().enumerate() {
            buffer[i * 4..(i + 1) * 4].copy_from_slice(&val.to_ne_bytes());
        }

        // [16..20]: stroke width
        buffer[16..20].copy_from_slice(&self.width.to_ne_bytes());

        // [20..32]: zero padding for 16-byte struct alignment
        buffer[20..32].fill(0);
    }

    fn requires_backdrop(&self) -> bool {
        false
    }
}

impl IntoProp<Material> for Stroke {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::collect([self]))
    }
}

impl IntoProp<Overlay> for Stroke {
    fn into_prop(self) -> Prop<Overlay> {
        Prop::Static(Overlay::collect([self]))
    }
}
