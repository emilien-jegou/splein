// Single responsibility: Wave-1 DSL upgrade repro — P4 (constant pose props
// are dropped), P2 (`{rows}` vec splices interpose a wrapper group), P3
// (components have no first-class `key`).
//
// One variable per section; human judges each prediction, no test asserts.
// Sections stay minimal: no motion_ui shells beyond the shared palette, so a
// layout delta cannot be blamed on chrome.

use super::motion_ui::*;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "riode — Wave 1 repro";
pub const HOT_WIDTH: u32 = 860;
pub const HOT_HEIGHT: u32 = 1020;
pub const HOT_BG: u32 = PAGE;

/// Pose travel in px: the static lanes sit at a constant offset; the dynamic
/// lanes ping-pong over the same range so "static vs dynamic" is one variable.
const POSE_TRAVEL: f32 = 40.0;

#[component]
pub fn Page() -> impl IntoElement {
    // Frame-clock ping-pong shared by the P4 dynamic lanes (moves from frame 0).
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 8.0) / 8.0;
        (if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        }) as f32
    });
    let y_dyn = t.map(|v| v * POSE_TRAVEL);
    let y_dyn2 = t.map(|v| v * POSE_TRAVEL);

    // P3 shuffle: reverses the chip order. Chips are intentionally unkeyed —
    // press counts sticking to *position* is the baseline this section shows.
    let flipped = use_signal(false);
    let order = flipped.map(|f| {
        if f {
            vec!["delta", "charlie", "test", "alpha"]
        } else {
            vec!["alpha", "bravo", "charlie", "delta"]
        }
    });

    view! {
        <Group direction="vertical" width="fill" height="fill" fill={PAGE}>
            <Group
                direction="vertical"
                width="fill"
                gap={6.0}
                margin={Margin::sides(28.0, 28.0, 20.0, 28.0)}
            >
                <Eyebrow label="UPGRADE · WAVE 1 REPRO" />
                <Text size={24.0} weight={700u16} color={INK} selectable={true}>
                    "P4 pose · P2 splice · P3 key"
                </Text>
                <Text size={13.0} line_height={20.0} color={BODY} width="fill" selectable={true}>
                    "One variable per section. F8 freezes, '.' steps, '[' ']' slow time. Human judges; no test asserts."
                </Text>
            </Group>
            <Group width="fill" height={1.0} fill={hairline()} />

            <Group direction="vertical" width="fill" height="fill" margin={Margin::all(24.0)} gap={4.0}>
                <Section
                    title="P4 · CONSTANT POSE"
                    note="Left: y={40.0} literal. Middle: y from ticker. Right: static x=10 + dynamic y on one box."
                >
                    <Group direction="horizontal" width="fill" gap={12.0}>
                        <PoseLane label="STATIC Y={40}">
                            <Group
                                width={36.0}
                                height={36.0}
                                radius={8.0}
                                fill={ACCENT}
                                margin={Margin::left(12.0)}
                                y={40.0}
                            />
                        </PoseLane>
                        <PoseLane label="DYNAMIC Y (TICKER)">
                            <Group
                                width={36.0}
                                height={36.0}
                                radius={8.0}
                                fill={ACCENT}
                                margin={Margin::left(12.0)}
                                y={y_dyn}
                            />
                        </PoseLane>
                        <PoseLane label="STATIC X=10 + DYN Y">
                            <Group
                                width={36.0}
                                height={36.0}
                                radius={8.0}
                                fill={CHIP}
                                margin={Margin::left(12.0)}
                                x={10.0}
                                y={y_dyn2}
                            />
                        </PoseLane>
                    </Group>
                </Section>

                <Section
                    title="P2 · VEC SPLICE"
                    note="Same three boxes twice. A splices a Vec via {rows}; B hands the same vec to .children(). They must look identical."
                >
                    <Group direction="horizontal" width="fill" gap={20.0}>
                        <Group direction="vertical" width="fill">
                            <Eyebrow label="A · RSX {rows} SPLICE" />
                            <Group direction="vertical" width="fill" gap={12.0} margin={Margin::top(8.0)}>
                                {p2_boxes()}
                            </Group>
                        </Group>
                        <Group direction="vertical" width="fill">
                            <Eyebrow label="B · BUILDER .children(rows)" />
                            {p2_stack()}
                        </Group>
                    </Group>
                </Section>

                <Section
                    title="P3 · COMPONENT KEY"
                    note="Click chips to accumulate presses, then SHUFFLE both lists. Left: counts stick to position. Right: <for key=…> keys the count state to the label (P1) and tag key= gives reconciler node identity (P3) — counts follow the label."
                >
                    <Group direction="horizontal" width="fill" gap={20.0}>
                        <Group direction="vertical" width="fill" gap={8.0}>
                            <Eyebrow label="UNKEYED (BASELINE)" />
                            <for id in order.clone()>
                                <Chip id={id} />
                            </for>
                        </Group>
                        <Group direction="vertical" width="fill" gap={8.0}>
                            <Eyebrow label="KEYED (FOR-KEY + TAG-KEY)" />
                            <for id in order key={|s| s.to_string()}>
                                <Chip key={id} id={id} />
                            </for>
                        </Group>
                    </Group>
                    <Group
                        width="fill"
                        height={36.0}
                        radius={10.0}
                        fill={CHIP}
                        alignment="center"
                        distribution="center"
                        margin={Margin::top(10.0)}
                        on:click={{ move |_| flipped.update(|f| *f = !*f) }}
                    >
                        <Text size={12.0} weight={700u16} color={Color::WHITE}>
                            "SHUFFLE ORDER"
                        </Text>
                    </Group>
                </Section>
            </Group>
        </Group>
    }
}

/// Section block: eyebrow, one-line note, then the section body.
///
/// Builder-bodied on purpose: an RSX `{children}` splice would itself route
/// through the F1 wrapper-group bug (P2) and make the demo's own chrome an
/// uncontrolled variable.
#[component]
fn Section(title: &'static str, note: &'static str, children: Vec<Element>) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .margin(Margin::top(14.0))
        .children((
            Eyebrow(title),
            text(note).size(12.0).color(MUTED).width("fill"),
            group()
                .direction("vertical")
                .width("fill")
                .margin(Margin::top(8.0))
                .children(children),
        ))
}

/// Fixed-height track holding one P4 pose box (the box is the section child).
#[component]
fn PoseLane(label: &'static str, children: Vec<Element>) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .children((
            Eyebrow(label),
            group()
                .width("fill")
                .height(96.0)
                .radius(12.0)
                .fill(SURFACE)
                .stroke(Stroke::inside(1.0, hairline()))
                .margin(Margin::top(6.0))
                .children(children),
        ))
}

/// The three P2 boxes, built twice (once spliced, once builder-forwarded) so
/// both columns carry identical content.
fn p2_boxes() -> Vec<Element> {
    let mk = |label: &'static str, fill: u32| {
        view! {
            <Group
                direction="horizontal"
                width="fill"
                height={32.0}
                radius={8.0}
                fill={fill}
                alignment="center"
                margin={Margin::x(10.0)}
            >
                <Text size={12.0} weight={600u16} color={Color::WHITE}>{label}</Text>
            </Group>
        }
    };
    vec![
        mk("box one", ACCENT),
        mk("box two", BODY),
        mk("box three", CHIP),
    ]
}

/// Column B: the same boxes handed straight to the parent's children setter
/// (the flat, wrapper-free path the splice should match).
fn p2_stack() -> GroupDef {
    group()
        .direction("vertical")
        .width("fill")
        .gap(12.0)
        .margin(Margin::top(8.0))
        .children(p2_boxes())
}

/// One P3 chip: press count lives in this component's own hook, which is what
/// makes positional identity visible on shuffle.
#[component]
fn Chip(id: &'static str) -> impl IntoElement {
    let count = use_signal(0u32);
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={40.0}
            radius={10.0}
            fill={SURFACE}
            stroke={Stroke::inside(1.0, hairline())}
            alignment="center"
            on:click={{ move |_| count.update(|n| *n += 1) }}
        >
            <Group
                width={10.0}
                height={10.0}
                radius="max"
                fill={ACCENT}
                margin={Margin::left(12.0)}
            />
            <Text size={13.0} weight={600u16} color={INK} margin={Margin::left(10.0)}>
                {id}
            </Text>
            <Group width="fill" height="fill" />
            <Text size={12.0} weight={700u16} color={ACCENT}>
                {count.map(|n| format!("{n} presses"))}
            </Text>
            <Group width={12.0} height="fill" />
        </Group>
    }
}
