use riode::dsl::IntoElement;
use riode::foundation::{EffectStage, Geo, Stage};
use riode::prelude::*;
use riode_shader::{Blend, Fill, Shape};

pub const HOT_TITLE: &str = "Riode — Post-Processing & Blending Showcase";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 560;
pub const HOT_BG: u32 = 0x11111B;

// ============================================================================
// 1. UNMISSABLE EFFECT: Inverts entire subtree (Blue -> Yellow, White -> Black)
// ============================================================================
#[derive(Clone, Copy, Debug)]
pub struct InvertGlowEffect;

pub const INVERT_EFFECT_WGSL: &str = r#"
fn effect_apply_color(src_col: vec4<f32>, px: vec2<f32>, geo: ElementGeo) -> vec4<f32> {
    if (src_col.a <= 0.0) {
        return src_col;
    }
    let straight = src_col.rgb / src_col.a;
    // High-contrast color inversion:
    // Pure blue (0.0, 0.0, 1.0) -> Pure Bright Yellow (1.0, 1.0, 0.0)!
    // White text (1.0, 1.0, 1.0) -> Pitch Black text (0.0, 0.0, 0.0)!
    let inv = vec3<f32>(1.0 - straight.r, 1.0 - straight.g, 1.0 - straight.b);
    return vec4<f32>(inv * src_col.a, src_col.a);
}
"#;

impl EffectStage for InvertGlowEffect {
    fn wgsl_source(&self) -> &'static str {
        INVERT_EFFECT_WGSL
    }

    fn uniform_size(&self) -> usize {
        0
    }

    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
}

// ============================================================================
// 2. UNMISSABLE OVERLAY: Smooth Anti-Aliased 4px Gold Inner Border over Squircle
// ============================================================================
#[derive(Clone, Copy, Debug)]
pub struct GoldBorderOverlay;

pub const OVERLAY_WGSL: &str = r#"
fn stage_apply(payload: ptr<function, FragmentPayload>) {
    // Clipped strictly to the SDF shape boundary
    if ((*payload).dist > 0.0) {
        return;
    }

    // Anti-aliased 4px inner border along the squircle boundary
    let border_coverage = smoothstep(-4.5, -3.5, (*payload).dist);
    let border_rgb = vec3<f32>(1.0, 0.84, 0.0); // Bright Gold

    let src = vec4<f32>(border_rgb * border_coverage, border_coverage);
    (*payload).color = src + (*payload).color * (1.0 - src.a);
}
"#;

impl Stage for GoldBorderOverlay {
    fn wgsl_source(&self) -> &'static str {
        OVERLAY_WGSL
    }

    fn uniform_size(&self) -> usize {
        0
    }

    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
}

// ============================================================================
// 3. UI Showcase
// ============================================================================
#[component(name = "demo_effects::Page")]
pub fn Page() -> impl IntoElement {
    build_ui()
}

pub fn build_ui() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            material={Fill::solid(0x11111B)}
            padding={Padding::all(28.0)}
            gap={24.0}
            alignment="center"
            distribution="center"
        >
            // Header
            <Group direction="vertical" alignment="center" gap={6.0}>
                <Text size={24.0} weight={700} color={Color::WHITE}>
                    "Shader-Level Post-Processing & Blending"
                </Text>
                <Text size={13.0} color={0xA6ADC8}>
                    "Live subtrees distorted by post-composite shaders and non-linear blend modes"
                </Text>
            </Group>

            // Row of Cards
            <Group direction="horizontal" gap={28.0} alignment="center" distribution="center">

                // Card 1: Subtree Effect applied directly to the container
                // Declared: Blue background + White text
                // Expected with Effect: Bright Yellow background + Black text
                <Group
                    direction="vertical"
                    width={210.0}
                    height={240.0}
                    shape={Shape::rounded(24.0)}
                    material=[Fill::solid(0x0000FF)] // Pure Blue
                    effect=[InvertGlowEffect]        // Inverts to Pure Yellow
                    padding={Padding::all(20.0)}
                    alignment="center"
                    distribution="space_between"
                >
                    <Text size={12.0} weight={600} color={Color::WHITE}>
                        "POST-EFFECT"
                    </Text>
                    <Text size={20.0} weight={700} color={Color::WHITE}>
                        "Inverted Card"
                    </Text>
                    <Text size={12.0} color={Color::WHITE}>
                        "Turned yellow by Effect"
                    </Text>
                </Group>

                // Card 2: Overlay running AFTER children, clipped to Squircle
                <Group
                    direction="vertical"
                    width={210.0}
                    height={240.0}
                    shape={Shape::squircle(32.0)}
                    material=[
                        Fill::solid(0x831843), // Berry pink surface
                    ]
                    overlay=[
                        GoldBorderOverlay, // 4px Gold inner border contouring squircle
                    ]
                    padding={Padding::all(20.0)}
                    alignment="center"
                    distribution="space_between"
                >
                    <Text size={12.0} weight={600} color={0xCDD6F4}>
                        "SURFACE + OVERLAY"
                    </Text>
                    <Text size={20.0} weight={700} color={Color::WHITE}>
                        "Squircle Card"
                    </Text>
                    <Text size={12.0} color={0xF5C2E7}>
                        "Gold border overlay"
                    </Text>
                </Group>

                // Card 3: Subtree Screen Blend & Leaf Additive Glow
                <Group
                    direction="vertical"
                    width={210.0}
                    height={240.0}
                    shape={Shape::rounded(24.0)}
                    material=[
                        Fill::solid(0x064E3B), // Emerald green
                    ]
                    effect=[Blend::Screen]
                    padding={Padding::all(20.0)}
                    alignment="center"
                    distribution="space_between"
                >
                    <Text size={12.0} weight={600} color={0xA6E3A1}>
                        "SUBTREE BLEND"
                    </Text>
                    <Text size={20.0} weight={700} color={Color::WHITE}>
                        "Screen Composite"
                    </Text>
                    <Text size={14.0} weight={700} color={0x94E2D5} blend={BlendMode::Additive}>
                        "Additive Glow Text"
                    </Text>
                </Group>

            </Group>

            // Footer
            <Group
                shape={Shape::rounded(12.0)}
                material={Fill::solid(0x1E1E2E)}
                padding={Padding::symmetric(10.0, 20.0)}
            >
                <Text size={12.0} color={0x9399B2}>
                    "Zero fixed-function GPU blend state permutations — 100% shader-evaluated"
                </Text>
            </Group>
        </Group>
    }
}
