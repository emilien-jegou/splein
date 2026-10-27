// Single responsibility: minimal custom-shader probe — one opaque card.
//
// One Group wearing a hand-written `CustomEffect` (no DSL, no sliders):
// if the custom stage runs, the card paints solid red with rounded
// corners; if the stage is skipped (pipeline fail, no card, wrong
// engine) the card is transparent/missing. Run with wgpu — tiny-skia
// ignores `material` by design.

use riode::foundation::{BackdropNeed, CustomEffect};
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both.
pub const HOT_TITLE: &str = "Shader probe";
pub const HOT_WIDTH: u32 = 900;
pub const HOT_HEIGHT: u32 = 600;
pub const HOT_BG: u32 = 0x14161C;

/// Simplest opaque card: constant red radiance, alpha from the CARD
/// (`ctx.color.a` — the host's rounded silhouette). `ctx.coverage` is
/// 1.0 across the whole scissor rect and would paint a square plaque,
///
/// Hand-written twin of what the DSL `lower_to_apply` emits for a
/// constant term: client prepends the ABI header, glue appends fs_main.
const SOLID_RED: &str = r#"
fn apply(
    ctx: EffectCtx,
    backdrop: texture_2d<f32>,
    sharp: texture_2d<f32>,
    u: CustomUniforms,
) -> EffectCtx {
    var out = ctx;
    let card_a = ctx.color.a;
    let radiance = vec3<f32>(1.0, 0.0, 0.0);
    out.color = vec4<f32>(radiance * card_a, card_a);
    return out;
}
"#;

#[component]
pub fn Page() -> impl IntoElement {
    // G1d cursor probe (NEW STEP code): a signal-fed value inside the
    // DSL block. `cursor.get()` is a runtime value shape (method call
    // that is not a material terminal ⇒ `Binding::Value` ⇒ slot), so the
    // block emits a `move ||` Dynamic prop: every write re-runs ONLY the
    // filler (one `queue.write_buffer` of 128 B), the WGSL source stays
    // byte-identical, the source-keyed cache must hit. The probe maps
    // pointer-x to a `Sky::gain` multiplier: park left = dim plaque,
    // drag right = full-bright red. Shape must never move (host-owned).
    let cursor = use_signal(0.5f32);
    view! {
        <Group
            width="fill"
            height="fill"
            fill={HOT_BG}
            alignment="center"
            distribution="center"
        >
            <Group
                width={400.0}
                height={300.0}
                radius={24.0}
                material=[CustomEffect::new(SOLID_RED, Vec::new(), BackdropNeed::None)]
                alignment="center"
                distribution="center"
            >
                <Text size={48.0} color={Color::WHITE}>{"Aa"}</Text>
            </Group>
            // DSL basis row (Phase 1 G1a): bare-Color degenerate ambient,
            // visibility-scaled ×30 so the 1.3% GGX lobe reads on screen.
            // Same host, same shape as the raw square above. Saturated red
            // would prove the interpreter never ran (the macro would have
            // passed the block through); full black would prove the stage
            // dropped (neutral host showing through).
            <Group direction="vertical" gap={8.0} alignment="center">
                <Group
                    width={400.0}
                    height={300.0}
                    radius={24.0}
                    material={let c = Color::RED * Intensity::factor(30.0); (c).into_surface()}
                    alignment="center"
                    distribution="center"
                >
                    <Text size={48.0} color={Color::WHITE}>{"Aa"}</Text>
                </Group>
                <Text size={13.0} color={0x9AA0AE}>{"DSL: bare-Color degenerate ambient ×30 — mid red plaque expected"}</Text>
            </Group>
            // Geo plumbing row (Phase 2 G1b): the SAME degenerate lobe,
            // but the `mix` weight reads `geo.edge` — the card silhouette
            // alpha the custom pass genuinely receives per pixel (the
            // host's rounded shape). Center must render flat (edge ≈ 1
            // inside the card ⇒ red side wins); corners must fall toward
            // teal as the AA'd alpha drops. Same shape as the rows above
            // (silhouette comes from the host either way — §1.3 opacity
            // rule); only the per-pixel RADIANCE varies.
            <Group direction="vertical" gap={8.0} alignment="center">
                <Group
                    width={400.0}
                    height={300.0}
                    radius={24.0}
                    material={|geo| { mix(Color::RED * Intensity::factor(30.0), Color::rgb(0.0, 0.5, 0.5) * Intensity::factor(30.0), geo.position()).into_surface() }}
                    // NOTE: double braces — the XML attr layer strips the
                    // outer brace pair, so the closure needs its own block
                    // braces to hold statements (single-expr form needs
                    // none, as above; G1a stays single-tail `let`-block).
                    // `geo.position` = length(ctx.px): a full-card radial
                    // sweep (visible everywhere), proving per-pixel host
                    // variation tracks geometry. `geo.edge` (1.0 - card
                    // alpha) lives only in the ~1px AA fringe — correct but
                    // invisible at a glance, so it stays the fringe
                    // definition, not the demo signal.
                    alignment="center"
                    distribution="center"
                >
                    <Text size={48.0} color={Color::WHITE}>{"Aa"}</Text>
                </Group>
                <Text size={13.0} color={0x9AA0AE}>{"DSL |geo|: edge-driven mix — flat center, teal falloff at corners"}</Text>
            </Group>
                    // Cursor-reactive row (G1d): same degenerate lobe, but lit by
            // `Sky::gain(cursor)` — one slot refilled from the pointer.
            // ×30 visibility gain so the sweep reads on screen; cursor-x
            // maps 0.05–2.0 (dim-left → bright-right); builds stay flat
            // while dragging (D-M5 two-keys).
            <Group direction="vertical" gap={8.0} alignment="center">
                <Group
                    width={400.0}
                    height={260.0}
                    radius={24.0}
                    hit_test={HitTest::Opaque}
                    on:pointer-move={{
                        let cursor = cursor;
                        move |info: &PointerEvent| {
                            cursor.set((info.position.x / 900.0).clamp(0.05, 2.0));
                        }
                    }}
                    material={
                        let c = Color::RED * Intensity::factor(30.0);
                        let lobe = Microfacet::ggx(Normal::flat(), 1.0);
                        (c * lobe * Sky::gain(cursor.get())).into_surface()
                    }
                    alignment="center"
                    distribution="center"
                >
                    <Text size={48.0} color={Color::WHITE}>{"Aa"}</Text>
                </Group>
                <Text size={13.0} color={0x9AA0AE}>{"DSL reactive: drag across the card — brightness tracks x, shape never moves"}</Text>
            </Group>
        </Group>
    }
    .into_element()
}
