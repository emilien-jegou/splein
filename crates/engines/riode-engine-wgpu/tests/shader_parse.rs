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
