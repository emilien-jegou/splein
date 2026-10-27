// Single responsibility: Phase 5 visual instrument — the 6 liquid-glass
// reference cards (5f) plus one verification row per landed batch
// (5b refraction/frost, 5c surface light, 5d volume, 5e fields/light).
//
// Every card wears a real DSL `Surface` (auto-stamped neutral host +
// post-glass custom stage — D-M4) over hard-stop color stripes, so
// displacement, blur, tint and glint all read against hue edges. Static:
// F8 freeze + `s` capture, zoom to compare. Human judges; no test asserts.
//
// Per AGENTS.md the predictions below are for the human to check, not
// claims. A black/missing card means the stage dropped (backdrop need or
// pipeline miss); a flat card means the normal isn't feeding the closure;
// an identical pair means a uniform isn't bound.

use riode::foundation::{ColorStop, Fill, LinearGradient, Point};
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Liquid glass lab — Phase 5 instrument";
pub const HOT_WIDTH: u32 = 1120;
pub const HOT_HEIGHT: u32 = 860;
pub const HOT_BG: u32 = 0xD8D8DA;

const INK: u32 = 0x1C2333;
const SUB: u32 = 0x4A5262;
const CARD_W: f32 = 310.0;
const CARD_H: f32 = 200.0;

// ---------------------------------------------------------------------------
// Stage: hard-stop stripes so refraction/displacement read against hue edges.
// ---------------------------------------------------------------------------

fn stripes() -> Fill {
    Fill::LinearGradient(LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(1120.0, 0.0),
        stops: vec![
            ColorStop { position: 0.0, color: Color::rgb(0.90, 0.90, 0.92) },
            ColorStop { position: 0.2, color: Color::rgb(0.90, 0.90, 0.92) },
            ColorStop { position: 0.2, color: Color::rgb(0.85, 0.20, 0.20) },
            ColorStop { position: 0.4, color: Color::rgb(0.85, 0.20, 0.20) },
            ColorStop { position: 0.4, color: Color::rgb(0.20, 0.70, 0.30) },
            ColorStop { position: 0.6, color: Color::rgb(0.20, 0.70, 0.30) },
            ColorStop { position: 0.6, color: Color::rgb(0.20, 0.40, 0.90) },
            ColorStop { position: 0.8, color: Color::rgb(0.20, 0.40, 0.90) },
            ColorStop { position: 0.8, color: Color::rgb(0.90, 0.90, 0.92) },
            ColorStop { position: 1.0, color: Color::rgb(0.90, 0.90, 0.92) },
        ],
    })
}

// ---------------------------------------------------------------------------
// 5b verification: refraction, frost, ribbed lens.
// ---------------------------------------------------------------------------

/// Droplet lens (ex 9): steep edge-distance dome, sharp, IOR 1.33.
fn surf_droplet() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 2.4);
    Interface::dielectric(n, 1.33)
        .transmit(0.0)
        .tint(Color::WHITE)
        .reflect(0.01)
        .lit_by(1.0)
        .into_surface()
}

/// Fluted glass (ex 2): vertical sine ribs drive the normal, slight frost,
/// chromatic fan at the rib flanks.
fn surf_fluted() -> Surface {
    let n = Normal::from_height(Field::sine(0.08, true), 1.0);
    Interface::dielectric(n, 1.55)
        .transmit(0.02)
        .disperse(0.04)
        .tint(Color::rgb(0.92, 0.96, 1.0))
        .reflect(0.02)
        .lit_by(1.0)
        .into_surface()
}

/// Frost wash: flat normal, IOR 1.0 (no bend), pure 5b kernel blur.
fn surf_frost() -> Surface {
    Transmit::sample()
        .normal(Normal::flat())
        .ior(1.0)
        .frost(0.35)
        .tint(Color::rgb(0.90, 0.93, 0.96))
        .into_surface()
}

// ---------------------------------------------------------------------------
// 5c verification: dielectric interface, roughness response.
// ---------------------------------------------------------------------------

/// Acrylic OFF: opaque plaque baseline (no PBR math — must read flat).
fn surf_off() -> Surface {
    let m: MaterialExpr = Color::rgb(0.08, 0.08, 0.10).into();
    m.into_surface()
}

/// Acrylic ON (ex 1): frosted transmit + tinted dielectric sheen.
fn surf_on() -> Surface {
    let n = Normal::flat();
    let bg = Transmit::sample()
        .normal(n.clone())
        .ior(1.0)
        .frost(0.45)
        .tint(Color::rgb(0.08, 0.08, 0.10))
        .build();
    let sheen = Specular::ggx(n, 0.35).lit_by(0.4).build();
    (bg + sheen).into_surface()
}

/// Rough HI: same stack shape as ON but a broad matte lobe (compare the
/// highlight width against ON's tight sheen).
fn surf_rough_hi() -> Surface {
    let n = Normal::flat();
    let bg = Transmit::sample()
        .normal(n.clone())
        .ior(1.5)
        .frost(0.05)
        .tint(Color::rgb(0.75, 0.78, 0.85))
        .build();
    let coat = Specular::ggx(n, 0.6).lit_by(1.2).build();
    (bg + coat).into_surface()
}

// ---------------------------------------------------------------------------
// 5d verification: Beer volume over a depth sweep.
// ---------------------------------------------------------------------------

/// Clear slab: interface, no attenuation (baseline for the smoked pair).
fn surf_clear() -> Surface {
    Interface::dielectric(Normal::flat(), 1.5)
        .transmit(0.05)
        .tint(Color::WHITE)
        .into_surface()
}

/// Smoked obsidian (ex 3): bevel normal, dense beer attenuation.
fn surf_smoked() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 8.0);
    Interface::dielectric(n, 1.62)
        .transmit(0.05)
        .attenuate(Color::rgb(0.10, 0.04, 0.02), 3.5)
        .reflect(0.005)
        .lit_by(1.5)
        .into_surface()
}

/// Deep smoke: same shape, double density (rim must densify vs SMOKED).
fn surf_deep() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 8.0);
    Interface::dielectric(n, 1.62)
        .transmit(0.05)
        .attenuate(Color::rgb(0.10, 0.04, 0.02), 6.0)
        .reflect(0.005)
        .lit_by(1.5)
        .into_surface()
}

// ---------------------------------------------------------------------------
// 5e verification: procedural fields, emission, virtual lights.
// ---------------------------------------------------------------------------

/// Frosted ice (ex 5): lens + noise microstructure, wet clearcoat mixed by
/// Schlick weight.
fn surf_ice() -> Surface {
    let n_lens = Normal::from_height(Field::edge_dist(), 1.5);
    let n_micro = Normal::from_height(Field::noise(4.0), 1.0);
    let rough_refract = Transmit::sample()
        .normal(n_lens.clone() + n_micro.clone())
        .ior(1.31)
        .frost(0.35)
        .tint(Color::rgb(0.85, 0.95, 1.0))
        .build();
    let wet_coat = Specular::ggx(n_lens.clone(), 0.05).lit_by(1.0).build();
    mix(rough_refract, wet_coat, fresnel(n_lens.clone(), 1.31)).into_surface()
}

/// Neon HUD (ex 7): dim transmit + grid-masked cyan emission + coat.
fn surf_hud() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 1.5);
    let bg = Transmit::sample()
        .normal(n.clone())
        .ior(1.45)
        .frost(0.05)
        .build();
    let glow = Emission::glow(Color::rgb(0.0, 1.0, 0.9), 2.5)
        .mask(Field::grid(16.0, 1.0))
        .build();
    let coat = Specular::ggx(n, 0.02).lit_by(1.0).build();
    (bg + glow + coat).into_surface()
}

/// Cursor glint (ex 15): studio key + fixed-anchor pointer lamp on a bevel.
/// The lamp anchor is a baked card-local position (150, 100); per-frame
/// `geo.pointer_px()` routing is Phase 9e work — this row proves the two
/// highlights sum with different directions, not live tracking.
fn surf_glint() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 8.0);
    let key = Lights::directional((-0.3, -0.7, 0.8));
    let lamp = Lights::point_xy(150.0, 100.0, 50.0);
    let a = Specular::ggx(n.clone(), 0.05).lit_by_light(key).build();
    let b = Specular::ggx(n.clone(), 0.05).lit_by_light(lamp).build();
    (a + b).into_surface()
}

// ---------------------------------------------------------------------------
// 5f references: the 6 historical models, each on landed builders only.
// ---------------------------------------------------------------------------

/// 1. LiquidGlassKit: squircle edge refraction, 3-tap dispersion, glare coat.
fn surf_kit() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 2.0);
    Interface::dielectric(n, 1.52)
        .transmit(0.05)
        .disperse(0.04)
        .tint(Color::WHITE)
        .reflect(0.02)
        .lit_by(1.2)
        .into_surface()
}

/// 2. adaptive_liquid_glass: frosted wash + rim streak + warm touch glow.
fn surf_adaptive() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 1.5);
    let wash = Transmit::sample()
        .normal(n.clone())
        .ior(1.40)
        .frost(0.30)
        .tint(Color::rgb(0.88, 0.92, 0.97))
        .build();
    let rim = Specular::ggx(n, 0.10).lit_by(1.0).build();
    let touch = Emission::glow(Color::rgb(1.0, 0.6, 0.3), 1.5)
        .mask(Field::sine(0.05, false))
        .build();
    (wash + rim + touch).into_surface()
}

/// 3. liquid-glass Voice pill: dense slab + bevel + fixed pointer lamp.
fn surf_pill() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 6.0);
    Interface::dielectric(n, 1.50)
        .transmit(0.04)
        .attenuate(Color::rgb(0.20, 0.45, 0.85), 2.0)
        .reflect(0.01)
        .lit_by_light(Lights::point_xy(160.0, 36.0, 60.0))
        .into_surface()
}

/// 4. apple-glass SDF meniscus: dome curvature + directional key light.
fn surf_sdf() -> Surface {
    let n = Normal::from_height(Field::edge_dist(), 3.0);
    let film = Transmit::sample()
        .normal(n.clone())
        .ior(1.40)
        .frost(0.02)
        .tint(Color::WHITE)
        .build();
    let key = Specular::ggx(n, 0.08)
        .lit_by_light(Lights::directional((0.4, -0.6, 0.7)))
        .build();
    (film + key).into_surface()
}

/// 5. apple-glass SVG displacement: layered sine + noise ripple normals.
fn surf_ripple() -> Surface {
    let ripples = Normal::from_height(Field::sine(0.05, false), 0.6)
        + Normal::from_height(Field::noise(2.0), 0.8);
    Transmit::sample()
        .normal(ripples)
        .ior(1.33)
        .frost(0.0)
        .tint(Color::WHITE)
        .into_surface()
}

/// 6. decant: high-density translucency + tight clearcoat glint.
fn surf_decant() -> Surface {
    let n = Normal::flat();
    let body = Interface::dielectric(n.clone(), 1.50)
        .transmit(0.02)
        .attenuate(Color::rgb(0.05, 0.10, 0.12), 4.0)
        .reflect(0.005)
        .lit_by(2.0)
        .build();
    let glint = Specular::ggx(n.clone(), 0.01).lit_by(2.0).build();
    (body + glint).into_surface()
}

// ---------------------------------------------------------------------------
// Layout: one labelled card per surface, three per row, scrolling column.
// ---------------------------------------------------------------------------

#[component]
fn Card(label: &'static str, caption: &'static str, surface: Surface) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={8.0} alignment="center">
            <Group
                width={CARD_W}
                height={CARD_H}
                radius={24.0}
                material={surface}
                alignment="center"
                distribution="center"
            >
                <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{label}</Text>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{caption}</Text>
        </Group>
    }
}

#[component]
fn PillCard(label: &'static str, caption: &'static str, surface: Surface) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={8.0} alignment="center">
            <Group
                width={CARD_W}
                height={72.0}
                radius="max"
                material={surface}
                alignment="center"
                distribution="center"
            >
                <Text size={15.0} weight={700u16} color={Color::hex(INK)}>{label}</Text>
            </Group>
            <Text size={11.0} weight={600u16} color={Color::hex(SUB)}>{caption}</Text>
        </Group>
    }
}

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group
            width="fill"
            height="fill"
            fill={stripes()}
            clip={true}
            alignment="center"
        >
            <ScrollView width="fill" height="fill">
                <Group direction="vertical" gap={28.0} margin={32.0} alignment="center">
                    <Text size={22.0} weight={800u16} color={Color::hex(INK)}>{"LIQUID GLASS LAB · PHASE 5"}</Text>
                    <Text size={12.0} color={Color::hex(SUB)}>{"freeze F8 · step . · capture s · human verdict per AGENTS.md"}</Text>
                    <Group direction="horizontal" gap={24.0} alignment="center">
                        <Card label="DROPLET" caption="5b · bands bend to center" surface={surf_droplet()} />
                        <Card label="FLUTED" caption="5b · vertical rib lens + fringe" surface={surf_fluted()} />
                        <Card label="FROST" caption="5b · stripes blur, hue stays" surface={surf_frost()} />
                    </Group>
                    <Group direction="horizontal" gap={24.0} alignment="center">
                        <Card label="ACRYLIC OFF" caption="5c · flat plaque baseline" surface={surf_off()} />
                        <Card label="ACRYLIC ON" caption="5c · depth + tight sheen" surface={surf_on()} />
                        <Card label="ROUGH .6" caption="5c · broad matte vs ON" surface={surf_rough_hi()} />
                    </Group>
                    <Group direction="horizontal" gap={24.0} alignment="center">
                        <Card label="CLEAR SLAB" caption="5d · no-attenuation baseline" surface={surf_clear()} />
                        <Card label="SMOKED" caption="5d · darkens, rim holds" surface={surf_smoked()} />
                        <Card label="DEEP SMOKE" caption="5d · denser than SMOKED" surface={surf_deep()} />
                    </Group>
                    <Group direction="horizontal" gap={24.0} alignment="center">
                        <Card label="FROSTED ICE" caption="5e · microstructure sparkle" surface={surf_ice()} />
                        <Card label="NEON HUD" caption="5e · cyan grid glow lines" surface={surf_hud()} />
                        <Card label="GLINT" caption="5e · key + lamp hotspots" surface={surf_glint()} />
                    </Group>
                    <Group direction="horizontal" gap={24.0} alignment="center">
                        <Card label="LIQUIDGLASSKIT" caption="ref 1 · edge + dispersion" surface={surf_kit()} />
                        <Card label="ADAPTIVE" caption="ref 2 · wash + rim + glow" surface={surf_adaptive()} />
                        <PillCard label="VOICE PILL" caption="ref 3 · dense slab + lamp" surface={surf_pill()} />
                    </Group>
                    <Group direction="horizontal" gap={24.0} alignment="center">
                        <Card label="SDF MENISCUS" caption="ref 4 · dome + key light" surface={surf_sdf()} />
                        <Card label="SVG RIPPLE" caption="ref 5 · sine + noise lens" surface={surf_ripple()} />
                        <Card label="DECANT" caption="ref 6 · dense + glint" surface={surf_decant()} />
                    </Group>
                </Group>
            </ScrollView>
        </Group>
    }
    .into_element()
}
