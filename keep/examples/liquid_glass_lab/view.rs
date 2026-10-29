// Single responsibility: Step 2+3 gate instrument — Slab path length +
// Beer–Lambert Absorb through the in-place Tint upgrade (stage id 9),
// plus the Step 3 highlight pairing (rim + glint tiers of
// plan-better-glass-setup.md). Keeps the Step 1 gates (B1/B2) and adds
// the Step 2 absorb rows (C1/C2) and the Step 3 highlight rows (E2/E3;
// E1 is the chain-vs-preset pairing).
//
// Rows: B1 (static asymmetric corners), B2 (width-driven dynamic gate),
// C1 (LEFT flat tint vs RIGHT matched Absorb), C2 (capsule absorb),
// D1 (light spill off/on), E1 (preset knob-bag vs chain slice),
// E2 (rim tier: canonical vs folded item), E3 (glint tier off/on).
// A cursor-following saturated disc paints BEHIND the cards so the lens
// has chroma to refract wherever the human parks it — the white/grey
// stripe field alone hides the lens. Static: F8 freeze + `s` capture,
// zoom to compare. Human judges; no test asserts.
//
// Per AGENTS.md the predictions below are for the human to check, not
// claims.

use riode::foundation::{ColorStop, Fill, LightSpill, LinearGradient, Point, Radius};
use riode::internal::group;
use riode::prelude::*;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Liquid glass lab — Step 2+3 gates";
pub const HOT_WIDTH: u32 = 1120;
pub const HOT_HEIGHT: u32 = 2340;
pub const HOT_BG: u32 = 0xD8D8DA;

const INK: u32 = 0x1C2333;
const SUB: u32 = 0x4A5262;
const CARD_W: f32 = 310.0;
const CARD_H: f32 = 200.0;

// ---------------------------------------------------------------------------
// Cursor probe: a saturated disc that follows the pointer exactly,
// painted BEHIND the glass cards (negative z stacking context) so the
// lens has saturated chroma to refract wherever the human parks it.
// White/grey stripes hide the lens; a live disc under the cards exposes
// displacement, blur and dispersion against a hard hue edge.
//
// `probe_at` holds window-space cursor px. The root group catches every
// `pointer-move` (bubbling: moves over cards still reach it) and stores
// the position; the disc translates so its centre sits on the cursor
// (`-R` offset folds the top-left anchor). `HitTest::Passthrough` keeps
// the disc out of the hit chain — it never steals moves from the root.
// No scroll container on this page, so window space == layout space and
// the disc sits exactly under the cursor.
// ---------------------------------------------------------------------------

const PROBE_R: f32 = 90.0;

/// Saturated magenta disc, centred on `at` (window px), behind siblings.
fn probe_disc(at: Signal<Point>) -> Element {
    let pos = at.map(move |p| Transform::from_translation(p.x - PROBE_R, p.y - PROBE_R));
    group()
        .width(PROBE_R * 2.0)
        .height(PROBE_R * 2.0)
        .radius("max")
        .absolute()
        .z_index(-10)
        .hit_test(HitTest::Passthrough)
        .fill(Color::hex(0xE548E8))
        .transform(pos)
        .into_element()
}

// ---------------------------------------------------------------------------
// Stage: hard-stop stripes so refraction/displacement read against hue edges.
// ---------------------------------------------------------------------------

fn stripes() -> Fill {
    Fill::LinearGradient(LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(1120.0, 0.0),
        stops: vec![
            ColorStop {
                position: 0.0,
                color: Color::rgb(0.90, 0.90, 0.92),
            },
            ColorStop {
                position: 0.2,
                color: Color::rgb(0.90, 0.90, 0.92),
            },
            ColorStop {
                position: 0.2,
                color: Color::rgb(0.85, 0.20, 0.20),
            },
            ColorStop {
                position: 0.4,
                color: Color::rgb(0.85, 0.20, 0.20),
            },
            ColorStop {
                position: 0.4,
                color: Color::rgb(0.20, 0.70, 0.30),
            },
            ColorStop {
                position: 0.6,
                color: Color::rgb(0.20, 0.70, 0.30),
            },
            ColorStop {
                position: 0.6,
                color: Color::rgb(0.20, 0.40, 0.90),
            },
            ColorStop {
                position: 0.8,
                color: Color::rgb(0.20, 0.40, 0.90),
            },
            ColorStop {
                position: 0.8,
                color: Color::rgb(0.90, 0.90, 0.92),
            },
            ColorStop {
                position: 1.0,
                color: Color::rgb(0.90, 0.90, 0.92),
            },
        ],
    })
}

// ---------------------------------------------------------------------------
// Step 1 (plan-better-glass-setup): per-corner radii through the lens SDF
// + morph track. B1 is a static asymmetric-corner row; B2 is the dynamic
// gate — one row whose geometry is signal-driven pill <-> asymmetric.
// ---------------------------------------------------------------------------

/// B1: asymmetric corners (`Radius::corners`) on the same stripe field.
/// The per-quadrant SDF must match the card's own rounded-rect clip
/// exactly: no flattening, no quadrant seams. Overlap pair merges
/// smoothly (`smin` intact).
#[component]
fn B1AsymmetricRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={Radius::corners(64.0, 8.0, 64.0, 8.0)}
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"B1 ASYMM"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"B1 · [64,8,64,8] silhouette = clip"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={Radius::corners(64.0, 8.0, 64.0, 8.0)}
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"B1 UNION"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"B1 · union pair, smooth neck"}</Text>
                </Group>
            </Group>
        </Group>
    }
}

/// B2: the dynamic-geometry gate. `GroupDef.radius` is a static
/// `Radius` (signal-driven radius is Step-4 value plumbing), so the gate
/// drives geometry the reactive way today's tree allows: an animated
/// WIDTH signal. Layout re-resolves corners every tick — the Max pill
/// and the fixed asymmetric set both restamp per frame through the
/// 4-component morph track with zero pipeline rebuilds.
#[component]
fn B2MorphRow() -> impl IntoElement {
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 4.0) / 4.0;
        let tri = if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        };
        tri as f32
    });
    let w: Computed<f32> = t.map(|k| 160.0 + 160.0 * k);
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={w.clone()}
                        height={72.0}
                        radius="max"
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"B2 PILL"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"B2 · width-driven pill, restamps/frame"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={w.clone()}
                        height={72.0}
                        radius={Radius::corners(48.0, 8.0, 48.0, 8.0)}
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"B2 ASYMM"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"B2 · width-driven asymm, corners glide"}</Text>
                </Group>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"B2 · bevel glued while morphing, no recompile"}</Text>
        </Group>
    }
}

/// Step 2 (plan-better-glass-setup.md): Slab + Absorb. LEFT is today's
/// flat tint, RIGHT is `Absorb` (Beer–Lambert) matched at card centre —
/// same hue, same strength, density on. The dye must concentrate where
/// the slab is thick (bevel band), not wash uniformly.
const ABSORB_TINT: u32 = 0x1E6FD9;
const ABSORB_ALPHA: f32 = 0.35;
const ABSORB_DENSITY: f32 = 0.02;

#[component]
fn C1AbsorbRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={GlassPreset::regular().style_tinted(Color::hex_alpha(ABSORB_TINT, ABSORB_ALPHA)).into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"C1 FLAT"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"C1 · flat tint, no density"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={{
                            let mut s = GlassPreset::regular().style_tinted(Color::hex_alpha(ABSORB_TINT, ABSORB_ALPHA));
                            s.backdrop = s.backdrop.absorb_density(ABSORB_DENSITY);
                            s.into_effects()
                        }}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"C1 ABSORB"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"C1 · Beer–Lambert, density on"}</Text>
                </Group>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"C1 · dye peaks in the bevel, centre stays clear glass (blur + wash, no dye)"}</Text>
        </Group>
    }
}

/// C2: capsule absorb — the slab is deepest along the long bevel, so
/// the dye gradient must follow the capsule's long axis.
#[component]
fn C2CapsuleRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group
                width={CARD_W + 110.0}
                height={72.0}
                radius="max"
                material={{
                    let mut s = GlassPreset::regular().style_tinted(Color::hex_alpha(ABSORB_TINT, ABSORB_ALPHA));
                    s.backdrop = s.backdrop.absorb_density(ABSORB_DENSITY);
                    s.into_effects()
                }}
                alignment="center"
                distribution="center"
            >
                <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"C2 CAPSULE ABSORB"}</Text>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"C2 · dye follows the long bevel"}</Text>
        </Group>
    }
}

/// E1: the 4-stage Chain slice vs the old preset look. LEFT is today's
/// `GlassPreset::regular()` (the knob-bag path); RIGHT is
/// `ChainGlass::regular()` (Refract → Frost → Beer → Fresnel, fitted
/// numbers, explicit 4-item fold). Same card size, same stripes, same
/// probe disc — the only difference is which path builds the material.
///
/// Per AGENTS.md the predictions below are for the human to check, not
/// claims.
#[component]
fn E1ChainRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"E1 OLD"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E1 · preset knob-bag path"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={riode::foundation::glass::ChainGlass::regular().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"E1 NEW"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E1 · Chain 4-stage slice"}</Text>
                </Group>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E1 · RIGHT must read as glass: bend + frost + dye + edge glint, band tracks geometry"}</Text>
        </Group>
    }
}

/// E2: the rim tier pairing (Step 3 highlight story, plan-better-glass-setup.md).
/// LEFT is the canonical preset path — `stageRim` runs there with the
/// fitted bank knobs (width 1.2454, intensity 0.2315, back-rim 0.35
/// baked as the bank constant, light −3π/4 as the frame global). RIGHT
/// is the chain path, where the rim rider `ChainDirectionalRim`
/// previously did NOT run: its knobs sat on the lens but stage id 12 was
/// absent from the explicit fold's dispatch. The new `Rim()` unit item
/// makes the fold run the same shader with the same knobs.
///
/// Per AGENTS.md the predictions below are for the human to check, not
/// claims.
#[component]
fn E2RimRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"E2 PRESET"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E2 · LEFT canonical — the streak runs"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={riode::foundation::glass::ChainGlass::regular().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"E2 CHAIN"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E2 · RIGHT folded rim item — same streak"}</Text>
                </Group>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E2 · both cards: bright upper-left streak, dimmer back edge; lengths read identical"}</Text>
        </Group>
    }
}

/// E3: the GGX glint pairing. Both cards are the chain path; RIGHT adds
/// the glint tier by gain (`ChainGlass::glint(..)` → `Ggx()` folded
/// item + knobs). The bevel gate `smoothstep(0.08, 0.6, vv)` and the
/// key light (−3π/4) are baked in `ggx.wgsl` — nothing to author.
#[component]
fn E3GlintRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={riode::foundation::glass::ChainGlass::regular().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"E3 GLINT OFF"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E3 · LEFT chain regular — glint 0"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={riode::foundation::glass::ChainGlass::regular().glint(2.0).into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"E3 GLINT ON"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E3 · RIGHT authored glint gain 2.0"}</Text>
                </Group>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"E3 · a bevel-glued glint on the upper-left edge; flat face untouched"}</Text>
        </Group>
    }
}

/// Step 3 (plan-better-glass-setup.md): light-spill replay — the soft
/// pastel light a card spills onto the background beneath its bottom
/// edge. Same card, same lens, spill OFF vs ON: the ON card replays a
/// blurred copy of the finished card in the strip below the edge only,
/// dimmer than the card itself. Renderer-side replay (renderer.rs
/// `light_spill_replay`), not a glass stage — so there is no bevel
/// brightening and no halo around the card, only the below-edge strip.
#[component]
fn D1SpillRow() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={10.0} alignment="center">
            <Group direction="horizontal" gap={24.0} alignment="center">
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={GlassPreset::regular().style().into_effects()}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"D1 NO SPILL"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"D1 · spill off, bare edge"}</Text>
                </Group>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Group
                        width={CARD_W}
                        height={CARD_H}
                        radius={24.0}
                        material={{
                            GlassPreset::regular().style()
                                .with_light_spill(LightSpill::new(0.6).length(64.0))
                                .into_effects()
                        }}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{"D1 SPILL"}</Text>
                    </Group>
                    <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"D1 · soft light below the edge"}</Text>
                </Group>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{"D1 · downward strip only, dimmer than the card, no halo"}</Text>
        </Group>
    }
}

#[component]
pub fn Page() -> impl IntoElement {
    // Cursor probe state: window-space pointer position, updated on every
    // move anywhere over the page (the handler lives on the root group so
    // bubbling moves over cards/rows still land here). Starts parked over
    // the first card row so the lens shows colour on the first frame.
    let probe_at = use_signal(Point::new(560.0, 200.0));
    view! {
        <Group
            width="fill"
            height="fill"
            fill={stripes()}
            clip={true}
            alignment="center"
            hit_test={HitTest::Opaque}
            on:pointer-move={{
                let probe_at = probe_at;
                move |info: &PointerEvent| {
                    probe_at.set(info.position);
                }
            }}
        >
            {probe_disc(probe_at)}
            <Group
                direction="vertical"
                width="fill"
                height="fill"
                gap={28.0}
                margin={32.0}
                alignment="center"
                distribution="center"
            >
                <Text size={22.0} weight={800u16} color={Color::hex(INK)}>{"LIQUID GLASS LAB · STEP 2+3"}</Text>
                <Text size={12.0} color={Color::hex(SUB)}>{"freeze F8 · step . · capture s · human verdict per AGENTS.md"}</Text>
                <B1AsymmetricRow />
                <B2MorphRow />
                <C1AbsorbRow />
                <C2CapsuleRow />
                <D1SpillRow />
                <E1ChainRow />
                <E2RimRow />
                <E3GlintRow />
            </Group>
        </Group>
    }
    .into_element()
}
