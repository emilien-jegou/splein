// Single responsibility: Variable-font shaping identity and DSL support.

use aurora::dsl::text;
use aurora::foundation::Constraints;
use aurora::text::{FontVariation, TextConfig, TextContext};
use std::sync::Arc;

fn context() -> TextContext {
    let ctx = TextContext::new();
    ctx.load_font(include_bytes!("../assets/inter-var.ttf"));
    ctx
}

fn config(content: &str) -> TextConfig {
    TextConfig {
        content: content.to_string(),
        size: 32.0,
        ..TextConfig::default()
    }
}

#[test]
#[ntest::timeout(5_000)]
fn variation_participates_in_cache_identity() {
    let ctx = context();
    let mut cfg = config("Aurora");
    cfg.variations = vec![FontVariation::named("wght", 700.0).unwrap()];
    let first = ctx.shape_config(&cfg, Constraints::UNCONSTRAINED);
    let second = ctx.shape_config(&cfg, Constraints::UNCONSTRAINED);
    assert!(
        Arc::ptr_eq(&first, &second),
        "identical variations must hit the cache"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn dsl_exposes_font_variations() {
    let def = text("Aurora")
        .font_variation("wght", 700.0)
        .font_variations(&[("opsz", 14.0)]);
    let cfg = def.to_config("Aurora".to_string());
    assert_eq!(cfg.variations.len(), 2);
    // Unknown tags are ignored instead of breaking the chain.
    let def = text("Aurora").font_variation("toolong", 1.0);
    assert!(def.to_config("Aurora".to_string()).variations.is_empty());
    assert!(FontVariation::named("wght", 1.0).is_some());
    assert!(FontVariation::named("w", 1.0).is_none());
}
