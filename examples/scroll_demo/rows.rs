// Responsibility: Scroll-demo row compositions — clock-driven drift
// viewports, the ScrollView / VirtualScrollView runs, and the selectable-text
// card. The two restore cases under review here: `thumb_fill` inks the
// scrollbar thumb, `selection_fill` inks selected text.

use riode::internal::*; // P10 escape hatch: rows compose repr-builders directly
use riode::prelude::*;
use riode_shader::Fill;

use super::chrome::{selection_ink, thumb_ink, EyebrowProps, INK, SURFACE};

/// Travel in px per axis (less than content overflow so clamp never engages).
pub const TRAVEL: f32 = 160.0;

/// One labelled clipped viewport holding a pre-shifted content element.
#[component]
pub fn ScrollRow(label: &'static str, content: Element) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={110.0}
            alignment="center"
            gap={16.0}
        >
            <Group width={96.0} height="fill" alignment="center">
                <Eyebrow label={label} />
            </Group>
            <Group
                width="fill"
                height="fill"
                material={Fill::solid(SURFACE)}
                clip={true}
            >
                {content}
            </Group>
        </Group>
    }
}

/// A controlled `ScrollView` auto-scrolling end to end (see `Page`). This is
/// the scrollbar watch: the thumb fill must appear at the top-right edge
/// while content moves and fade out when the scroll rests.
#[component]
pub fn ScrollViewRow(label: &'static str, handle: ScrollHandle) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={110.0}
            alignment="center"
            gap={16.0}
        >
            <Group width={96.0} height="fill" alignment="center">
                <Eyebrow label={label} />
            </Group>
            <Group width="fill" height="fill" clip={true}>
                <ScrollView
                    width={Size::fill()}
                    height={Size::fill()}
                    handle={handle}
                    thumb_fill={thumb_ink()}
                >
                    {scroll_content("view")}
                </ScrollView>
            </Group>
        </Group>
    }
}

/// A manually-driven 100k-row virtualized viewport (see `Page`). Same thumb
/// contract, plus wheel/drag/key input instead of the ticker.
#[component]
pub fn VirtualScrollRow(label: &'static str, handle: ScrollHandle) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={110.0}
            alignment="center"
            gap={16.0}
        >
            <Group width={96.0} height="fill" alignment="center">
                <Eyebrow label={label} />
            </Group>
            <Group width="fill" height="fill" clip={true}>
                <VirtualScrollView
                    width={Size::fill()}
                    height={Size::fill()}
                    len={100_000usize}
                    row_height={36.0}
                    handle={handle}
                    thumb_fill={thumb_ink()}
                    row={vrow}
                />
            </Group>
        </Group>
    }
}

/// A padded surface card of selectable text: drag across the lines to judge
/// the live highlight, double-click for a word, `Ctrl+C` to copy, click
/// elsewhere to clear.
#[component]
pub fn SelectionRow(label: &'static str) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={150.0}
            alignment="center"
            gap={16.0}
        >
            <Group width={96.0} height="fill" alignment="center">
                <Eyebrow label={label} />
            </Group>
            <Group
                direction="vertical"
                width="fill"
                height="fill"
                padding={18.0}
                gap={6.0}
                material={Fill::solid(SURFACE)}
            >
                <SelectionLine text="Drag across these lines — one highlight band per line, behind the glyphs." />
                <SelectionLine text="Double-click a word for a whole-word grab; hold and sweep across nodes." />
                <SelectionLine text="Release keeps the band; Ctrl+C reaches the OS clipboard; one click clears." />
                <SelectionLine text="Text without a selection_fill highlight stays selectable and paints nothing." />
            </Group>
        </Group>
    }
}

/// One selectable body line, the exact contract under review.
#[component]
pub fn SelectionLine(text: &'static str) -> impl IntoElement {
    view! {
        <Text
            size={14.0}
            line_height={20.0}
            color={INK}
            width="fill"
            selectable={true}
            selection_fill={selection_ink()}
        >{text}</Text>
    }
}

/// Tall striped content with numbered rows so motion and holes are visible.
pub fn scroll_content(axis: &str) -> Element {
    let rows: Vec<Element> = (0..24)
        .map(|i| {
            group()
                .width(1200.0)
                .height(48.0)
                .material(Fill::solid(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC }))
                .children([
                    text(format!("{axis} · row {i:02} — pack my box with five dozen liquor jugs"))
                        .size(14.0)
                        .color(INK),
                ])
                .into_element()
        })
        .collect();
    group()
        .direction("vertical")
        .width(1200.0)
        .height(1152.0)
        .children(rows)
        .into_element()
}

/// One virtualized row: uniform 36px, index-addressed so gaps show.
fn vrow(i: usize) -> Element {
    group()
        .width(Size::fill())
        .height(36.0)
        .material(Fill::solid(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC }))
        .children([
            text(format!("vrow {i:05} — pack my box with five dozen liquor jugs"))
                .size(14.0)
                .color(INK),
        ])
        .into_element()
}

/// The three drift runs the demo opens with: X on the SCROLL channel, Y on
/// the SCROLL channel, diagonal on PAINT (transform). Subpixel velocity.
pub fn drift_rows(x: Computed<f32>, y: Computed<f32>, xy: Computed<Transform>) -> Vec<Element> {
    vec![
        ScrollRow("X SCROLL", group().absolute().children([scroll_content("x")]).scroll_offset_x(x).into_element())
            .into_element(),
        ScrollRow("Y SCROLL", group().absolute().children([scroll_content("y")]).scroll_offset_y(y).into_element())
            .into_element(),
        ScrollRow("X+Y PAINT", group().absolute().children([scroll_content("xy")]).transform(xy).into_element())
            .into_element(),
    ]
}
