// Responsibility: Scroll-demo row compositions — clock-driven drift, ScrollView, VirtualScrollView, and selection card.

use riode::prelude::*;
use riode_shader::Fill;

use super::chrome::*;

pub const TRAVEL: f32 = 160.0;

#[component]
pub fn ScrollRow(label: &'static str, children: Vec<Element>) -> GroupElement {
    view! {
        <g row w="fill" h=110 align="center" gap=16>
            <g w=96 h="fill" align="center">
                <Eyebrow label={label} />
            </g>
            <g w="fill" h="fill" material={Fill::solid(SURFACE)} clip>
                {children}
            </g>
        </g>
    }
}

#[component]
pub fn ScrollViewRow(label: &'static str, handle: ScrollHandle) -> GroupElement {
    view! {
        <g row w="fill" h=110 align="center" gap=16>
            <g w=96 h="fill" align="center">
                <Eyebrow label={label} />
            </g>
            <g w="fill" h="fill" clip>
                <ScrollView
                    width="fill"
                    height="fill"
                    handle={handle}
                    thumb_fill={thumb_ink()}
                >
                    <ScrollContent axis="view" />
                </ScrollView>
            </g>
        </g>
    }
}

#[component]
pub fn VirtualScrollRow(label: &'static str, handle: ScrollHandle) -> GroupElement {
    view! {
        <g row w="fill" h=110 align="center" gap=16>
            <g w=96 h="fill" align="center">
                <Eyebrow label={label} />
            </g>
            <g w="fill" h="fill" clip>
                <VirtualScrollView
                    width="fill"
                    height="fill"
                    len={100_000usize}
                    row_height=36.0
                    handle={handle}
                    thumb_fill={thumb_ink()}
                    row={|i| view! { <VRow index={i} /> }}
                />
            </g>
        </g>
    }
}

#[component]
pub fn SelectionRow(label: &'static str) -> impl IntoElement {
    view! {
        <g row w="fill" h=150 align="center" gap=16>
            <g w=96 h="fill" align="center">
                <Eyebrow label={label} />
            </g>
            <g col w="fill" h="fill" p=18 gap=6 material={Fill::solid(SURFACE)}>
                <SelectionLine text="Drag across these lines — one highlight band per line, behind the glyphs." />
                <SelectionLine text="Double-click a word for a whole-word grab; hold and sweep across nodes." />
                <SelectionLine text="Release keeps the band; Ctrl+C reaches the OS clipboard; one click clears." />
                <SelectionLine text="Text without a selection_fill highlight stays selectable and paints nothing." />
            </g>
        </g>
    }
}

#[component]
pub fn SelectionLine(text: &'static str) -> impl IntoElement {
    view! {
        <text
            s=14
            line_height=20.0
            color={INK}
            width="fill"
            selectable
            selection_fill={selection_ink()}
        >{text}</text>
    }
}

#[component]
pub fn VRow(index: usize) -> impl IntoElement {
    let bg = if index % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC };
    let row_label = format!("vrow {index:05} — pack my box with five dozen liquor jugs");
    view! {
        <g w="fill" h=36.0 material={Fill::solid(bg)}>
            <text s=14 color={INK}>{row_label}</text>
        </g>
    }
}

#[component]
pub fn ScrollContent(axis: &'static str) -> impl IntoElement {
    view! {
        <g col w=1200.0 h=1152.0>
            <for i in 0..24>
                <g
                    w=1200.0
                    h=48.0
                    material={Fill::solid(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC })}
                >
                    <text s=14 color={INK}>
                        {format!("{axis} · row {i:02} — pack my box with five dozen liquor jugs")}
                    </text>
                </g>
            </for>
        </g>
    }
}

#[component]
pub fn DriftRows(x: Computed<f32>, y: Computed<f32>, xy: Computed<Transform>) -> impl IntoElement {
    view! {
        <g col w="fill" gap=16>
            <ScrollRow label="X SCROLL">
                <g absolute scroll_offset_x={x}>
                    <ScrollContent axis="x" />
                </g>
            </ScrollRow>
            <ScrollRow label="Y SCROLL">
                <g absolute scroll_offset_y={y}>
                    <ScrollContent axis="y" />
                </g>
            </ScrollRow>
            <ScrollRow label="X+Y PAINT">
                <g absolute transform={xy}>
                    <ScrollContent axis="xy" />
                </g>
            </ScrollRow>
        </g>
    }
}
