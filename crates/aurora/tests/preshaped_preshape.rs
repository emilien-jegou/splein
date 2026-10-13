// Build-time pre-computation: metrics equality between pre-shaped blobs
// and runtime shaping, plus SVG inclusion.
//
// The mirror contract (macros/src/preshape.rs): same parley shaping subset,
// same swash phase-0 raster, same hint cutoff. Advances and clusters must
// match EXACTLY; glyph origins may differ by the documented integer snap
// (<=0.5px). Any drift fails here — run after touching either side.

use aurora::foundation::Constraints;
use aurora::prelude::{include_svg, pre_shaped};
use aurora::text::{
    LineHeight, TextAlign, TextConfig, TextContext, TextOverflow, TextWrap,
};

fn context() -> TextContext {
    let ctx = TextContext::new();
    ctx.load_font(include_bytes!("../assets/inter-var.ttf"));
    ctx
}

fn runtime_layout(text: &str, size: f32) -> std::sync::Arc<aurora::text::TextLayout> {
    let ctx = context();
    ctx.shape_config(
        &TextConfig {
            content: text.to_string(),
            size,
            line_height: LineHeight::Absolute(size * 1.2),
            wrap: TextWrap::NoWrap,
            align: TextAlign::Start,
            overflow: TextOverflow::Clip,
            ..Default::default()
        },
        Constraints::UNCONSTRAINED,
    )
}

#[test]
fn preshaped_metrics_match_runtime() {
    let pre = pre_shaped!(
        font = "assets/inter-var.ttf",
        text = "Hello, World",
        size = 16.0
    );
    let ctx = context();
    let baked = ctx.shape_pre_shaped(&pre);
    let live = runtime_layout("Hello, World", 16.0);
    assert_eq!(baked.lines.len(), live.lines.len());
    assert_eq!(baked.total_size.height, live.total_size.height);
    for (bl, ll) in baked.lines.iter().zip(live.lines.iter()) {
        assert_eq!(bl.glyphs.len(), ll.glyphs.len());
        assert!((bl.width - ll.width).abs() <= 1.0, "widths: {} vs {}", bl.width, ll.width);
        for (bg, lg) in bl.glyphs.iter().zip(ll.glyphs.iter()) {
            assert_eq!(bg.glyph_id, lg.glyph_id, "glyph identity");
            assert_eq!(bg.advance, lg.advance, "advances exact");
            assert_eq!(bg.cluster, lg.cluster, "clusters exact");
            assert!(
                (bg.point.x - lg.point.x).abs() <= 0.5
                    && (bg.point.y - lg.point.y).abs() <= 0.5,
                "origins within integer snap: {:?} vs {:?}",
                bg.point,
                lg.point
            );
        }
    }
    // Bitmaps seeded into the shared cache under the synthetic keys.
    let stats = ctx.glyph_cache_stats();
    assert!(stats.entries > 0, "pre-shaped bitmaps seed the cache");
}

#[test]
fn preshaped_multiline_stacks() {
    let pre = pre_shaped!(
        font = "assets/inter-var.ttf",
        text = "One\nTwo",
        size = 16.0,
        line_height = 20.0
    );
    assert_eq!(pre.line_count(), 2);
    let layout = pre.to_layout();
    assert_eq!(layout.lines.len(), 2);
    assert_eq!(layout.total_size.height, 40.0);
}

#[test]
fn include_svg_rasterizes_red_square() {
    let img = include_svg!("assets/test-square.svg", 16, 16);
    assert_eq!((img.width, img.height), (16, 16));
    assert_eq!(img.data.len(), 16 * 16 * 4);
    // Solid red square: center pixel fully red opaque.
    let px = 8 * 16 * 4 + 8 * 4;
    assert_eq!(img.data[px], 255);
    assert_eq!(img.data[px + 1], 0);
    assert_eq!(img.data[px + 2], 0);
    assert_eq!(img.data[px + 3], 255);
}
