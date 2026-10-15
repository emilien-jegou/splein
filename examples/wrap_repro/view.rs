// Single responsibility: Wrap-rewrite repro — Fit-vs-wrap, FlowWrap chips,
// NoWrap, balance, aspect-ratio, and client AutoGrid in one page.
//
// Human verdict only; no test asserts on appearance.

use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this file composes repr-builders directly

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Wrap repro";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 900;
pub const HOT_BG: u32 = 0x10151D;

const LONG: &str = "Pack my box with five dozen liquor jugs and a few extra words to force wrapping inside fit containers";

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" height="fill" gap={12.0} padding="16">
            <ReproLabel label="1 · FIT TEXT IN FIT CONTAINER (must wrap at page edge, hug height)" />
            <Group width="fit" height="fit" fill="#1B2430" radius={10.0} padding="12">
                <Label copy={LONG} />
            </Group>
            <ReproLabel label="2 · CHIPS (Fit children, FlowWrap — must form lines, no 1-frame jitter)" />
            <Group direction="horizontal" width={340.0} height="fit" flow_wrap="wrap" gap={8.0} row_gap={8.0}>
                <Chip label="Submit" />
                <Chip label="Cancel the whole thing" />
                <Chip label="OK" />
                <Chip label="Remind me later" />
                <Chip label="Delete" />
                <Chip label="Save draft copy" />
            </Group>
            <ReproLabel label="3 · NOWRAP (single line, clipped — must not wrap or grow)" />
            <Group width={340.0} height="fit" fill="#1B2430" radius={10.0} padding="12" clip={true}>
                <NoWrapLabel copy={LONG} />
            </Group>
            <ReproLabel label="4 · BALANCE (heading — lines must be even, no orphan)" />
            <Group width={300.0} height="fit">
                <BalanceHeading copy="Balanced headings wrap evenly" />
            </Group>
            <ReproLabel label="5 · ASPECT RATIO (16/9 card — height follows width)" />
            <Group width={320.0} height="fit" aspect_ratio="16/9" fill="#24405E" radius={10.0}>
            </Group>
            <ReproLabel label="6 · CLIENT AUTOGRID (buttons change width — columns follow, no engine grid)" />
            <AutoGridDemo />
        </Group>
    }
}

#[component]
fn ReproLabel(label: &'static str) -> impl IntoElement {
    view! {
        <Group width="fill" height="fit">
            {text(label).size(12.0).color("#8B98A9")}
        </Group>
    }
}

#[component]
fn Label(copy: &'static str) -> impl IntoElement {
    view! {
        {text(copy).size(15.0).color("#FFFFFF")}
    }
}

#[component]
fn NoWrapLabel(copy: &'static str) -> impl IntoElement {
    view! {
        {text(copy).size(15.0).color("#FFFFFF").text_wrap("nowrap")}
    }
}

#[component]
fn BalanceHeading(copy: &'static str) -> impl IntoElement {
    view! {
        {text(copy).size(22.0).color("#FFFFFF").text_wrap("balance")}
    }
}

#[component]
fn Chip(label: &'static str) -> impl IntoElement {
    view! {
        <Group width="fit" height="fit" fill="#2B3A4D" radius="max" padding="8 14">
            {text(label).size(14.0).color("#FFFFFF")}
        </Group>
    }
}

#[component]
fn AutoGridDemo() -> impl IntoElement {
    let width = use_signal(420.0f32);
    let narrow = width;
    let widen = width;
    view! {
        <Group direction="vertical" width="fill" height="fit" gap={8.0}>
            <Group direction="horizontal" width="fit" height="fit" gap={8.0}>
                <Group width="fit" height="fit" fill="#2B3A4D" radius={8.0} padding="6 12" on_click={move |_| narrow.update(|v| *v = (*v - 40.0).max(160.0))}>
                    {text("−").size(14.0).color("#FFFFFF")}
                </Group>
                <Group width="fit" height="fit" fill="#2B3A4D" radius={8.0} padding="6 12" on_click={move |_| widen.update(|v| *v = (*v + 40.0).min(760.0))}>
                    {text("+").size(14.0).color("#FFFFFF")}
                </Group>
            </Group>
            <AutoGrid width={width} min_child={150.0} gap={8.0} />
        </Group>
    }
}

#[component]
fn AutoGrid(width: Signal<f32>, min_child: f32, gap: f32) -> impl IntoElement {
    let items = ["One", "Two", "Three", "Four", "Five", "Six"];
    let (cols, child_w) = riode::dsl::auto_grid::columns_for(width.get(), min_child, gap);
    let mut rows: Vec<Element> = Vec::new();
    for chunk in items.chunks(cols as usize) {
        let mut cells: Vec<Element> = Vec::new();
        for item in chunk {
            cells.push(
                group()
                    .width(child_w)
                    .height(48.0)
                    .fill("#2B3A4D")
                    .radius(8.0)
                    .alignment("center")
                    .children([text(*item).size(14.0).color("#FFFFFF").into_element()])
                    .into_element(),
            );
        }
        rows.push(
            group()
                .direction("horizontal")
                .width("fill")
                .height("fit")
                .gap(gap)
                .children(cells)
                .into_element(),
        );
    }
    group()
        .direction("vertical")
        .width(width.get())
        .height("fit")
        .gap(gap)
        .children(rows)
        .into_element()
}
