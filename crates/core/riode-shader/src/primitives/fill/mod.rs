// Single responsibility: Solid fill shader stage conforming to the Section 3 RFC contract.

use riode::foundation::{Color, Geo, Material, Stage};
use riode::reactive::prop::{IntoProp, Prop};

pub const FILL_WGSL: &str = r#"
fn stage_apply(payload: ptr<function, FragmentPayload>, uniforms: vec4<f32>) {
    let coverage = clamp(0.5 - (*payload).dist, 0.0, 1.0);
    let src = uniforms;
    
    // Premultiplied alpha blend over current payload color
    let out_a = src.a * coverage + (*payload).color.a * (1.0 - src.a * coverage);
    let out_rgb = src.rgb * src.a * coverage + (*payload).color.rgb * (1.0 - src.a * coverage);
    (*payload).color = vec4<f32>(out_rgb, out_a);
}
"#;

#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub color: Color,
}

impl Fill {
    pub fn new(color: impl Into<Color>) -> Self {
        Self {
            color: color.into(),
        }
    }

    pub fn solid(color: impl Into<Color>) -> Self {
        Self::new(color)
    }

    pub fn linear_gradient<I, C>(_angle_deg: f32, stops: I) -> Self
    where
        I: IntoIterator<Item = (f32, C)>,
        C: Into<Color>,
    {
        let mut iter = stops.into_iter();
        let color = iter.next().map(|(_, c)| c.into()).unwrap_or(Color::WHITE);
        Self { color }
    }
}

impl Stage for Fill {
    fn wgsl_source(&self) -> &'static str {
        FILL_WGSL
    }

    fn uniform_size(&self) -> usize {
        16 // vec4<f32> (RGBA)
    }

    fn write_uniforms(&self, _geo: &Geo, buffer: &mut [u8]) {
        let c = [self.color.r, self.color.g, self.color.b, self.color.a];
        for (i, val) in c.iter().enumerate() {
            buffer[i * 4..(i + 1) * 4].copy_from_slice(&val.to_ne_bytes());
        }
    }

    fn requires_backdrop(&self) -> bool {
        false
    }
}

impl IntoProp<Material> for Fill {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::collect([self]))
    }
}
