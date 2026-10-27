// Shader validity: every combined WGSL module the pipelines compile must
// parse under naga (catches structural breakage from stage refactors
// without needing a GPU device).
use riode::foundation::effect::shaders::{
    BLEND_COMPOSITE_COMBINED_WGSL, BLUR_WGSL, COMPOSITE_WGSL, GLASS_COMBINED_WGSL,
};

fn parse(name: &str, src: &str) {
    naga::front::wgsl::parse_str(src)
        .unwrap_or_else(|e| panic!("{name} failed to parse: {e:?}"));
}

#[test]
fn glass_combined_parses() {
    parse("glass", GLASS_COMBINED_WGSL);
}

#[test]
fn blend_composite_combined_parses() {
    parse("blend_composite", BLEND_COMPOSITE_COMBINED_WGSL);
}

#[test]
fn blur_parses() {
    parse("blur", BLUR_WGSL);
}

#[test]
fn composite_parses() {
    parse("composite", COMPOSITE_WGSL);
}

// Material-DSL lowering validity: build real Surfaces through the runtime
// vocabulary and parse the composed module (ABI + apply + engine entry)
// exactly as `custom_pipeline_for` does. A failure here = naga rejects the
// lowered WGSL = the stage drops and the card vanishes (`None` remembered).
use riode::foundation::effect::shaders::{EFFECT_ABI_WGSL, ENGINE_ENTRY_WGSL};
use riode::foundation::{Color, Intensity, Microfacet, Normal, Sky, Surface};
const SOLID_RAW: &str = "fn apply(ctx: EffectCtx, backdrop: texture_2d<f32>, sharp: texture_2d<f32>, u: CustomUniforms) -> EffectCtx { var out = ctx; let card_a = ctx.color.a; let radiance = vec3<f32>(1.0, 0.0, 0.0); out.color = vec4<f32>(radiance * card_a, card_a); return out; }";

fn parse_composed(name: &str, surface: &Surface) {
    let full = format!("{}\n{}\n{}", EFFECT_ABI_WGSL, surface.wgsl, ENGINE_ENTRY_WGSL);
    std::fs::write(format!("/tmp/probe_{name}.wgsl"), &full).ok();
    naga::front::wgsl::parse_str(&full)
        .unwrap_or_else(|e| panic!("{name} failed to parse: {e:?}"));
}

#[test]
fn material_g1a_degenerate_ambient_parses() {
    let c = Color::RED * Intensity::factor(30.0);
    parse_composed("g1a", &c.into_surface());
}

#[test]
fn material_g1d_reactive_lobe_parses() {
    use riode::foundation::fresnel;
    let c = Color::RED * Intensity::factor(30.0);
    let lobe = Microfacet::ggx(Normal::flat(), 1.0);
    parse_composed("g1d", &(c * lobe * Sky::gain(1.0)).into_surface());
}

#[test]
fn material_surfaces_validate_like_engine() {
    use riode::foundation::effect::shaders::{EFFECT_ABI_WGSL, ENGINE_ENTRY_WGSL};
    use riode::foundation::{Color, Intensity, Microfacet, Normal, Sky};
    let cases: Vec<(&str, String)> = vec![
        ("raw", SOLID_RAW.to_string()),
        ("g1a", {
            let c = Color::RED * Intensity::factor(30.0);
            let s = c.into_surface();
            assert!(!s.uniforms.is_empty());
            s.wgsl
        }),
        ("g1b", {
            use riode::foundation::{Geo, mix};
            let g = Geo::surface();
            mix(
                Color::RED * Intensity::factor(30.0),
                Color::rgb(0.0, 0.5, 0.5) * Intensity::factor(30.0),
                g.position(),
            ).into_surface().wgsl
        }),
        ("g1d", {
            let c = Color::RED * Intensity::factor(30.0);
            let lobe = Microfacet::ggx(Normal::flat(), 1.0);
            (c * lobe * Sky::gain(1.0)).into_surface().wgsl
        }),
    ];
    for (name, apply_src) in cases {
        let full = format!("{EFFECT_ABI_WGSL}\n{apply_src}\n{ENGINE_ENTRY_WGSL}");
        let module = naga::front::wgsl::parse_str(&full)
            .unwrap_or_else(|e| panic!("{name} parse failed: {e:?}"));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name} validate failed: {e:?}"));
    }
}
