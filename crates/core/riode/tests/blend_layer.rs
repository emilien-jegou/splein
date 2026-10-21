// Single responsibility: `BlendEffect` in an `effects()` stack reaches the
// wire — layer wrapping, DSL static/dynamic plumbing, stacking-context
// rule, and content-hash sensitivity to the mode. (Validated visually by
// the blend_group demo; these lock the plumbing in.)

use riode::dsl::*;
use riode::foundation::*;
use riode::reactive::*;
use riode::runtime::Engine;
use riode::scene::{command::hash_commands, SceneCommand};

fn blend_stack(mode: BlendMode) -> EffectStack {
    EffectStack::from(vec![BlendEffect::new(mode).boxed()])
}

/// Every `Blend` stage mode in the compiled scene's `BeginLayer` stacks,
/// in order.
fn layer_blends(engine: &Engine) -> Vec<BlendMode> {
    engine
        .scene()
        .chunks
        .iter()
        .flat_map(|c| c.commands.iter())
        .filter_map(|c| match c {
            SceneCommand::BeginLayer { effects, .. } => {
                effects.as_ref().and_then(|s| s.blend_mode())
            }
            _ => None,
        })
        .collect()
}

#[test]
#[ntest::timeout(5_000)]
fn static_blend_group_wraps_subtree_in_begin_layer_with_mode() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(
        group()
            .width(200.0)
            .height(200.0)
            .children([group()
                .width(50.0)
                .height(50.0)
                .fill(Color::RED)
                .effects(blend_stack(BlendMode::Multiply))]),
    );
    engine.frame();

    let blends = layer_blends(&engine);
    assert!(
        blends.contains(&BlendMode::Multiply),
        "blend group must emit BeginLayer with the mode on the wire, got {blends:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn plain_group_without_blend_emits_no_layer() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(
        group()
            .width(200.0)
            .height(200.0)
            .children([group().width(50.0).height(50.0).fill(Color::RED)]),
    );
    engine.frame();

    assert!(
        layer_blends(&engine).is_empty(),
        "a plain fill group must not create a layer"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn static_soft_light_reaches_wire() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(
        group()
            .width(200.0)
            .height(200.0)
            .children([group()
                .width(50.0)
                .height(50.0)
                .fill(Color::RED)
                .effects(blend_stack(BlendMode::SoftLight))]),
    );
    engine.frame();

    assert!(
        layer_blends(&engine).contains(&BlendMode::SoftLight),
        "soft-light blend effect must parse and reach the wire"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn dynamic_blend_change_reemits_layer_with_new_mode() {
    let mut engine = Engine::headless(200, 200);
    let mode = engine.signal(BlendMode::Multiply);
    let m = mode.clone();
    engine.mount(
        group()
            .width(200.0)
            .height(200.0)
            .children([group()
                .width(50.0)
                .height(50.0)
                .fill(Color::RED)
                .effects(computed(move || blend_stack(m.get())))]),
    );
    engine.frame();
    assert!(
        layer_blends(&engine).contains(&BlendMode::Multiply),
        "initial dynamic blend must reach the wire"
    );

    mode.set(BlendMode::Screen);
    engine.frame();
    let blends = layer_blends(&engine);
    assert!(
        blends.contains(&BlendMode::Screen) && !blends.contains(&BlendMode::Multiply),
        "changing the dynamic blend must re-emit the layer with the new mode, got {blends:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn content_hash_distinguishes_blend_modes() {
    use riode::scene::{LayerId, SceneChunk};

    let mk = |mode: BlendMode| {
        let mut chunk = SceneChunk::new(ResolvedRect::new(0.0, 0.0, 100.0, 100.0), Point::ZERO);
        chunk.push(SceneCommand::BeginLayer {
            id: LayerId(1),
            rect: ResolvedRect::new(0.0, 0.0, 50.0, 50.0),
            effects: Some(blend_stack(mode)),
            shadow_pad: 0.0,
        });
        chunk.push(SceneCommand::EndLayer { id: LayerId(1) });
        hash_commands(&chunk.commands)
    };

    assert_ne!(
        mk(BlendMode::Multiply),
        mk(BlendMode::Screen),
        "BeginLayer blend must feed the chunk content hash"
    );
    assert_eq!(
        mk(BlendMode::Overlay),
        mk(BlendMode::Overlay),
        "same mode must hash identically"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn appearance_hash_distinguishes_blend_only_difference() {
    use riode::scene::SceneChunk;

    let mk = |mode: BlendMode| {
        let mut appearance = Appearance::EMPTY.with_fill(Fill::Solid(Color::RED));
        appearance.effects = Some(blend_stack(mode));
        let mut chunk = SceneChunk::new(ResolvedRect::new(0.0, 0.0, 100.0, 100.0), Point::ZERO);
        chunk.push(SceneCommand::DrawRect {
            rect: ResolvedRect::new(0.0, 0.0, 10.0, 10.0),
            appearance,
        });
        hash_commands(&chunk.commands)
    };

    assert_ne!(
        mk(BlendMode::Normal),
        mk(BlendMode::Multiply),
        "appearance blend must feed the draw hash"
    );
}
