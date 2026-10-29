// Single responsibility: Interactive playground for the adaptive-glass
// material. View only (`Page` component, no `main`): the release driver
// (`main.rs`) and the hot entry (`hot.rs`) share this; the switchable
// studio reuses `boot` for in-window switching.
//
// Layout mirrors `shadow_studio`: control panel left (sliders bound to
// reactive signals + a preset cycler + a construction readout), preview
// stage right (two glass cards and a pill over drifting colour blobs, so
// the lens band, frost, tone and rim read clearly).

use riode::app::switcher::{BootCtx, BootedDemo};
use riode::foundation::{Color, ColorStop, EffectStack, Glass, GlassGroupEffect, GlassStyle, Glow, LinearGradient, Point, Refraction, Spectral};
use riode::internal::group;
use riode::prelude::*;
use riode::reactive::{Prop, ReactiveRuntime};
use std::cell::RefCell;
use std::rc::Rc;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Glass studio";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 700;
pub const HOT_BG: u32 = 0x14161C;

const TRACK_W: f32 = 200.0;
const TRACK_H: f32 = 28.0;
const THUMB_W: f32 = 14.0;

/// Userland looks under test: the laboratory opens on one of these,
/// sliders diverge from there. All numbers live in anodeui — the
/// engine never sees a preset name.
const PRESETS: [(fn() -> GlassPreset, &str); 4] = [
    (GlassPreset::regular as fn() -> GlassPreset, "anode: regular"),
    (GlassPreset::clear as fn() -> GlassPreset, "anode: clear"),
    (
        GlassPreset::regular_dark as fn() -> GlassPreset,
        "anode: regular dark",
    ),
    (
        GlassPreset::clear_dark as fn() -> GlassPreset,
        "anode: clear dark",
    ),
];

/// A labeled slider bound to `value` over `[min, max]`.
///
/// Relative dragging: the gesture anchors on grab, then maps horizontal
/// travel to the range, so no geometry reads are needed.
fn slider(
    rt: &Rc<RefCell<ReactiveRuntime>>,
    label: &'static str,
    value: Signal<f32>,
    min: f32,
    max: f32,
    fmt: Rc<dyn Fn(f32) -> String>,
) -> Element {
    let start_val = Signal::new(Rc::clone(rt), value.get());
    let start_x = Signal::new(Rc::clone(rt), 0.0f32);
    let thumb_x = computed(move || {
        let frac = ((value.get() - min) / (max - min)).clamp(0.0, 1.0);
        frac * (TRACK_W - THUMB_W)
    });
    let readout = computed({
        let fmt = Rc::clone(&fmt);
        move || fmt(value.get())
    });
    let track_value = value;
    let step_value = value;
    let start = start_val;
    let start_x_handle = start_x;
    view! {
        <Group direction="vertical" gap={4.0}>
            <Group direction="horizontal" gap={8.0} alignment="center">
                <Text size={13.0} color={0x9AA0AE}>{label}</Text>
                <Text size={13.0} color={0xFFFFFF}>{readout}</Text>
            </Group>
            <Group
                width={TRACK_W}
                height={TRACK_H}
                radius={6.0}
                fill={0x2A2E38}
                hit_test={HitTest::Opaque}
                on:drag-start={{
                    move |info: &DragInfo| {
                        start_val.set(track_value.get());
                        start_x.set(info.position.x);
                    }
                }}
                on:drag-step={{
                    move |info: &DragInfo| {
                        let travel = info.position.x - start_x_handle.get();
                        step_value.set(
                            (start.get() + travel * (max - min) / TRACK_W).clamp(min, max),
                        );
                    }
                }}
            >
                <Group
                    width={THUMB_W}
                    height={20.0}
                    radius={7.0}
                    fill={0x7DD3FC}
                    x={thumb_x}
                    y={4.0}
                    hit_test={HitTest::Passthrough}
                />
            </Group>
        </Group>
    }
    .into_element()
}

/// A small labeled button.
fn button(label: Prop<String>, on_press: impl Fn() + 'static) -> Element {
    view! {
        <Group
            width={200.0}
            height={34.0}
            radius={8.0}
            fill={0x2563EB}
            alignment="center"
            distribution="center"
            hit_test={HitTest::Opaque}
            on:click={{
                move |_| on_press()
            }}
        >
            <Text size={13.0} color={Color::WHITE}>{label}</Text>
        </Group>
    }
}

/// Every signal the studio owns, created explicitly on a runtime (`boot`) or
/// from hooks (`Page`). One struct so both entries build the same world.
/// Stage sliders diverge from the selected preset; reselecting a preset
/// snaps every slider back to its values.
#[derive(Clone)]
struct Params {
    preset_idx: Signal<i32>,
    frost: Signal<f32>,
    wash_alpha: Signal<f32>,
    saturation: Signal<f32>,
    tone_amt: Signal<f32>,
    ambient: Signal<f32>,
    rim_scale: Signal<f32>,
    strength: Signal<f32>,
    tint_alpha: Signal<f32>,
    corner_exp: Signal<f32>,
    show_glass: Signal<bool>,
    show_blobs: Signal<bool>,
    swapped: Signal<bool>,
}

/// Defaults track the regular anodeui preset.
fn params_on(runtime: &Rc<RefCell<ReactiveRuntime>>) -> Params {
    let mk = |v: f32| Signal::new(Rc::clone(runtime), v);
    let p = Params {
        preset_idx: Signal::new(Rc::clone(runtime), 0i32),
        frost: mk(0.0),
        wash_alpha: mk(0.0),
        saturation: mk(1.0),
        tone_amt: mk(1.0),
        ambient: mk(0.0),
        rim_scale: mk(1.0),
        strength: mk(1.0),
        tint_alpha: mk(0.0),
        corner_exp: mk(0.0),
        show_glass: Signal::new(Rc::clone(runtime), true),
        show_blobs: Signal::new(Rc::clone(runtime), true),
        swapped: Signal::new(Rc::clone(runtime), false),
    };
    apply_preset(&p, 0);
    p
}

/// Snap every stage slider to a preset's values: tint off, exponent
/// back to global.
fn apply_preset(p: &Params, idx: usize) {
    let preset = PRESETS[idx].0();
    let b = preset.backdrop;
    p.frost.set(b.blur.unwrap_or(0.0));
    p.wash_alpha.set(b.wash.flatten().map(|c| c.a).unwrap_or(0.0));
    p.saturation.set(b.saturation.unwrap_or(1.0));
    p.tone_amt.set(b.tone.unwrap_or(1.0));
    p.ambient.set(b.ambient_mix.unwrap_or(0.0));
    p.rim_scale.set(1.0);
    p.strength.set(1.0);
    p.tint_alpha.set(0.0);
    p.corner_exp.set(0.0);
}

/// The studio UI from already-created signals. Runs inside any build scope:
/// `boot` wraps it in `enter_keyed`, `Page` is a component scope already.
fn glass_fx(glass: Prop<GlassStyle>) -> Prop<EffectStack> {
    computed(move || glass.evaluate().into_effects())
}

fn ui(p: Params, rt: Rc<RefCell<ReactiveRuntime>>) -> Element {
    // The laboratory material: preset geometry + tone curve, every
    // stage from the sliders. Selecting a preset snaps the sliders;
    // moving a slider diverges from the preset.
    let glass = {
        let p = p.clone();
        computed(move || {
            let preset = PRESETS[p.preset_idx.get().clamp(0, 3) as usize].0();
            // Lens: preset geometry + runtime rim / strength / exponent.
            let mut g = preset.glass;
            g = g.rim(p.rim_scale.get().max(0.0));
            g.strength = p.strength.get().max(0.0);
            let exp = p.corner_exp.get();
            if exp > 0.0 {
                g.corner_exponent = Some(exp);
            }
            g = g.interactive(true);
            // Backdrop: preset stages (incl. its tone curve) + the
            // frost / wash / saturation / tone / ambient / tint sliders.
            let wash = preset
                .backdrop
                .wash
                .flatten()
                .unwrap_or(Color::rgba(0.0, 0.0, 0.0, 0.0));
            let mut b = preset
                .backdrop
                .blur(p.frost.get().max(0.0))
                .wash(Color::rgba(
                    wash.r,
                    wash.g,
                    wash.b,
                    p.wash_alpha.get().clamp(0.0, 1.0),
                ))
                .saturation(p.saturation.get().max(0.0))
                .tone(p.tone_amt.get().clamp(0.0, 1.0))
                .ambient(p.ambient.get().clamp(0.0, 1.0));
            let ta = p.tint_alpha.get().clamp(0.0, 1.0);
            if ta > 0.0 {
                b = b.tint(Color::hex_alpha(0x3B82F6, ta));
            }
            GlassStyle {
                glass: g,
                backdrop: b,
                spectral: Spectral::NONE,
                glow: Glow::NONE,
                refraction: Refraction::NONE,
                shadow: Some(preset.shadow.clone()),
            }
        })
    };
    let readout = {
        let p = p.clone();
        computed(move || {
            let idx = p.preset_idx.get().clamp(0, 3) as usize;
            format!(
                "{} · frost {:.2} · wash {:.2} · sat {:.2} · tone {:.2} · amb {:.2} · rim {:.2} · lens {:.2} · tint {:.2}",
                PRESETS[idx].1,
                p.frost.get(),
                p.wash_alpha.get(),
                p.saturation.get(),
                p.tone_amt.get(),
                p.ambient.get(),
                p.rim_scale.get(),
                p.strength.get(),
                p.tint_alpha.get(),
            )
        })
    };
    // Slow ping-pong driver for the backdrop blobs: the glass sits still,
    // the world moves under it — refraction must follow.
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 12.0) / 12.0;
        let tri = if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        };
        tri as f32
    });

    // Morph demo: the two MORPH pills swap flow spots (their `x`
    // offsets cross by exactly one pitch) while their `glass_id`s stay
    // put, so the layer glides them through each other on the morph
    // spring instead of snapping.
    let ax = {
        let v = p.swapped;
        computed(move || if v.get() { 230.0 } else { 0.0 })
    };
    let bx = {
        let v = p.swapped;
        computed(move || if v.get() { -230.0 } else { 0.0 })
    };

    let f2: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
    let f1: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.1}"));
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            fill={HOT_BG}
            alignment="center"
        >
            // Control panel: scrollable so sliders + toggles never overflow
            // the window height.
            <ScrollView width={248.0} height="fill">
            <Group direction="vertical" gap={10.0} margin={24.0}>
                {button(
                    {
                        let preset_idx = p.preset_idx;
                        computed(move || {
                            PRESETS[preset_idx.get().clamp(0, 3) as usize].1.to_string()
                        })
                    },
                    {
                        let p = p.clone();
                        move || {
                            let next = (p.preset_idx.get() + 1) % 4;
                            p.preset_idx.set(next);
                            apply_preset(&p, next as usize);
                        }
                    },
                )}
                {slider(&rt, "frost", p.frost, 0.0, 12.0, f2.clone())}
                {slider(&rt, "wash", p.wash_alpha, 0.0, 1.0, f2.clone())}
                {slider(&rt, "saturation", p.saturation, 0.0, 2.5, f2.clone())}
                {slider(&rt, "tone", p.tone_amt, 0.0, 1.0, f2.clone())}
                {slider(&rt, "ambient", p.ambient, 0.0, 1.0, f2.clone())}
                {slider(&rt, "rim", p.rim_scale, 0.0, 1.5, f2.clone())}
                {slider(&rt, "lens", p.strength, 0.0, 2.0, f2.clone())}
                {slider(&rt, "tint", p.tint_alpha, 0.0, 1.0, f2.clone())}
                {slider(&rt, "exponent", p.corner_exp, 0.0, 8.0, f1.clone())}
                {button(
                    {
                        let v = p.swapped;
                        computed(move || {
                            if v.get() {
                                "morph: SWAPPED (tap: restore)".to_string()
                            } else {
                                "morph: A·B (tap: swap)".to_string()
                            }
                        })
                    },
                    {
                        let v = p.swapped;
                        move || v.set(!v.get())
                    },
                )}
                {button(
                    {
                        let v = p.show_glass;
                        computed(move || {
                            if v.get() {
                                "glass: ON (tap: hide)".to_string()
                            } else {
                                "glass: OFF (tap: show)".to_string()
                            }
                        })
                    },
                    {
                        let v = p.show_glass;
                        move || v.set(!v.get())
                    },
                )}
                {button(
                    {
                        let v = p.show_blobs;
                        computed(move || {
                            if v.get() {
                                "blobs: ON (tap: hide)".to_string()
                            } else {
                                "blobs: OFF (tap: show)".to_string()
                            }
                        })
                    },
                    {
                        let v = p.show_blobs;
                        move || v.set(!v.get())
                    },
                )}
                // Fold: the panel stays fit to the sliders while this line
                // folds at whatever width the siblings decide — no fixed
                // width, no panel growth as the readout lengthens.
                <Text size={12.0} color={0x9AA0AE} width="fold">{readout}</Text>
            </Group>
            </ScrollView>
            // Preview stage.
            <Group
                width="fill"
                height="fill"
                fill={Fill::LinearGradient(LinearGradient {
                    start: Point::new(0.0, 0.0),
                    end: Point::new(800.0, 700.0),
                    stops: vec![
                        ColorStop {
                            position: 0.0,
                            color: Color::hex(0x1B2A6B),
                        },
                        ColorStop {
                            position: 1.0,
                            color: Color::hex(0x6B1B4A),
                        },
                    ],
                })}
                clip={true}
                alignment="center"
                distribution="center"
            >
                <if p.show_blobs.get()>
                {blob(
                    t.clone(),
                    "blob-blue",
                    130.0,
                    Color::hex_alpha(0x3B82F6, 0.85),
                    |v| Transform::from_translation(v * 480.0 + 40.0, 60.0),
                )}
                {blob(
                    t.clone(),
                    "blob-red",
                    100.0,
                    Color::hex_alpha(0xE5484D, 0.8),
                    |v| Transform::from_translation(560.0 - v * 480.0, 440.0),
                )}
                {blob(
                    t.clone(),
                    "blob-green",
                    80.0,
                    Color::hex_alpha(0x30A46C, 0.8),
                    |v| Transform::from_translation(v * 300.0 + 180.0, 520.0 - v * 60.0),
                )}
                </if>
                <if p.show_glass.get()>
                <Group
                    key="glass-cards"
                    direction="vertical"
                    gap={28.0}
                    alignment="center"
                    z_index={2}
                >
                    <Group direction="horizontal" gap={32.0} alignment="center">
                        <Group
                            width={220.0}
                            height={160.0}
                            radius={24.0}
                            material={glass_fx(glass.clone())}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={14.0} weight={700u16} color={Color::WHITE}>{"CARD"}</Text>
                        </Group>
                        <Group
                            width={160.0}
                            height={160.0}
                            radius="max"
                            material={glass_fx(glass.clone())}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={14.0} weight={700u16} color={Color::WHITE}>{"ORB"}</Text>
                        </Group>
                    </Group>
                    <Group
                        width={320.0}
                        height={64.0}
                        radius="max"
                        material={glass_fx(glass.clone())}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={15.0} weight={600u16} color={Color::WHITE}>{"Pill · live material"}</Text>
                    </Group>
                    <Group direction="vertical" gap={8.0} alignment="center">
                        <Text size={12.0} color={0x9AA0AE}>{"FLOW · spacing 40 bridges the 32 px gaps"}</Text>
                        <Group
                            direction="horizontal"
                            gap={32.0}
                            alignment="center"
                            material={EffectStack::from(vec![GlassGroupEffect::new(40.0).into_effect()])}
                        >
                            <Group
                                width={170.0}
                                height={96.0}
                                radius={28.0}
                                material={GlassPreset::regular().style().into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"FLOW A"}</Text>
                            </Group>
                            <Group
                                width={170.0}
                                height={96.0}
                                radius={28.0}
                                material={GlassPreset::clear().style().into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"FLOW B"}</Text>
                            </Group>
                            <Group
                                width={170.0}
                                height={96.0}
                                radius={28.0}
                                material={GlassPreset::regular().style_tinted(Color::hex_alpha(0x3B82F6, 0.85)).into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"FLOW C"}</Text>
                            </Group>
                        </Group>
                    </Group>
                    <Group direction="vertical" gap={8.0} alignment="center">
                        <Text size={12.0} color={0x9AA0AE}>{"UNION · one surface spans the 60 px gap"}</Text>
                        <Group
                            direction="horizontal"
                            gap={60.0}
                            alignment="center"
                            material={EffectStack::from(vec![GlassGroupEffect::new(8.0).into_effect()])}
                        >
                            <Group
                                width={170.0}
                                height={96.0}
                                radius={28.0}
                                material={GlassPreset::regular().style_union(7).into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"UNION A"}</Text>
                            </Group>
                            <Group
                                width={170.0}
                                height={96.0}
                                radius={28.0}
                                material={GlassPreset::regular().style_union(7).into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"UNION B"}</Text>
                            </Group>
                        </Group>
                    </Group>
                    <Group direction="vertical" gap={8.0} alignment="center">
                        <Text size={12.0} color={0x9AA0AE}>{"MORPH · swap crosses on the morph spring"}</Text>
                        <Group
                            direction="horizontal"
                            gap={60.0}
                            alignment="center"
                            material={EffectStack::from(vec![GlassGroupEffect::new(16.0).into_effect()])}
                        >
                            <Group
                                width={170.0}
                                height={96.0}
                                radius="max"
                                x={ax.clone()}
                                material={GlassPreset::regular().style_morph(21).into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"MORPH A"}</Text>
                            </Group>
                            <Group
                                width={170.0}
                                height={96.0}
                                radius="max"
                                x={bx.clone()}
                                material={GlassPreset::clear().style_morph(22).into_effects()}
                                alignment="center"
                                distribution="center"
                            >
                                <Text size={13.0} weight={700u16} color={Color::WHITE}>{"MORPH B"}</Text>
                            </Group>
                        </Group>
                    </Group>
                </Group>
                </if>
            </Group>
        </Group>
    }
    .into_element()
}

/// Rebuilds the studio world on a fresh runtime for in-window switching.
pub(crate) fn boot(ctx: &BootCtx) -> BootedDemo {
    let runtime = Rc::clone(&ctx.runtime);
    let p = params_on(&runtime);
    let cx = Cx::new(Rc::clone(&runtime));
    let root = cx
        .enter_keyed("root", move || {
            let rt = Rc::clone(&runtime);
            ui(p, rt)
        })
        .into_element();
    BootedDemo {
        root,
        background: HOT_BG.into(),
        extensions: vec![],
    }
}

/// One drifting colour blob, absolute (out of flow) like `liquid_glass`.
/// `key` gives the reconciler stable identity: without it, toggling a
/// sibling `<if>` branch reuses this node's slot positionally for a
/// different element (cards reconciled into an absolute node).
fn blob(
    t: Computed<f32>,
    key: &'static str,
    size: f32,
    fill: Color,
    place: impl Fn(f32) -> Transform + 'static,
) -> Element {
    group()
        .key(key)
        .width(size)
        .height(size)
        .radius("max")
        .absolute()
        .fill(fill)
        .transform(t.map(move |v| place(v)))
        .into_element()
}

/// Hot entry component: same world, hook-owned signals.
#[component]
pub fn Page() -> impl IntoElement {
    let p = Params {
        preset_idx: use_signal(0i32),
        frost: use_signal(5.9236),
        wash_alpha: use_signal(0.6839),
        saturation: use_signal(1.4901),
        tone_amt: use_signal(1.0),
        ambient: use_signal(0.18),
        rim_scale: use_signal(1.0),
        strength: use_signal(1.0),
        tint_alpha: use_signal(0.0),
        corner_exp: use_signal(0.0),
        show_glass: use_signal(true),
        show_blobs: use_signal(true),
        swapped: use_signal(false),
    };
    let rt = p.tint_alpha.runtime();
    ui(p, rt)
}
