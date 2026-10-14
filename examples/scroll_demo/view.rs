// Single responsibility: scroll repro — clip viewport + SCROLL-channel content drift.
//
// Rows 1-3 drive `.scroll_offset` (the SCROLL channel) from the frame clock:
// X-only, Y-only, diagonal. Row 4 mounts a real uncontrolled `ScrollView`
// (static until input lands) to prove mounting + post-layout clamp.
// Human judges: each `.` step advances rows 1-3; no holes at the leading
// edge; row 4 shows the top of its content with no overshoot frame.

use super::motion_ui::*;
use aurora::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Scroll drift repro";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 860;
pub const HOT_BG: u32 = PAGE;

/// Travel in px per axis (less than content overflow so clamp never engages).
const TRAVEL: f32 = 160.0;

/// Three SCROLL-channel drift runs plus one mounted `ScrollView`.
#[component]
pub fn Page() -> impl IntoElement {
    // Frame-clock ping-pong: 16s period (8s out, 8s back) => ~20px/s, subpixel per frame.
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 16.0) / 16.0;
        let tri = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
        tri as f32
    });

    let x = t.clone().map(|v| v * TRAVEL);
    let y = t.clone().map(|v| v * TRAVEL);
    let xy = t.map(|v| Transform::from_translation(-v * TRAVEL, -v * TRAVEL));

    // Row 4: a controlled ScrollView auto-scrolling end to end. The interval
    // writes the handle's offset signal every tick (ping-pong at the clamp
    // ends); the post-layout clamp publishes max and pulls the offset into
    // range. This is what fires SCROLL shifts + the raster blit.
    let sc = use_scroll(0.0);
    let dir = use_signal(1.0f32);
    {
        let sc = sc.clone();
        let dir = dir;
        use_interval(1.0 / 120.0, move || {
            let (cur, max) = (sc.offset.get(), sc.max.get());
            if cur >= max {
                dir.set(-1.0);
            } else if cur <= 0.0 {
                dir.set(1.0);
            }
            sc.scroll_by(dir.get() * 2.0);
        });
    }

    // Row 5: a virtualized 100k-row viewport, manually driven (no ticker:
    // auto-scroll would fight wheel/drag/key input). The window tracker
    // mounts only the visible band + overscan; sliding the window rebuilds
    // rows by index key without touching the rest.
    let vsc = use_scroll(0.0);

    view! {
        <Shell
            eyebrow_label="SCROLL · DRIFT + MOUNT"
            title="SCROLL channel drift plus auto-scrolling viewports."
            copy="Rows 1-3 drift at ~20px/s on the SCROLL/PAINT channels. Row 4 auto-scrolls a 24-row ScrollView; row 5 is a manually-driven 100k-row VirtualScrollView (windowed mount: wheel, drag, thumb, or click-then-keys). F8 freeze, '.' step: every press advances rows 1-3; watch row 5's leading edge for holes and its first column for index continuity."
        >
            <Content>
                <Group direction="vertical" width="fill" height="fill" gap={16.0}>
                    <ScrollRow
                        label="X SCROLL"
                        element={group().absolute().children([scroll_content("x")]).scroll_offset(x)}
                    />
                    <ScrollRow
                        label="Y SCROLL"
                        element={group().absolute().children([scroll_content("y")]).scroll_offset(y)}
                    />
                    <ScrollRow
                        label="X+Y PAINT"
                        element={group().absolute().children([scroll_content("xy")]).transform(xy)}
                    />
                    <ScrollViewRow label="SCROLLVIEW" handle={sc} />
                    <VirtualScrollRow label="VIRTUAL 100K" handle={vsc} />
                </Group>
            </Content>
        </Shell>
    }
}

/// One clipped viewport holding a pre-shifted absolute content element.
#[component]
fn ScrollRow(label: &'static str, element: Element) -> impl IntoElement {
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
                radius={12.0}
                fill={SURFACE}
                stroke={Stroke::inside(1.0, hairline())}
                clip={true}
            >
                {element}
            </Group>
        </Group>
    }
}

/// Row 4: a controlled `ScrollView` auto-scrolling end to end (see Page).
/// Proves mounting + clamp + SCROLL shifts + raster blit without any input.
#[component]
fn ScrollViewRow(label: &'static str, handle: ScrollHandle) -> impl IntoElement {
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
                radius={12.0}
                fill={SURFACE}
                stroke={Stroke::inside(1.0, hairline())}
                clip={true}
            >
                <ScrollView width={Size::fill()} height={Size::fill()} handle={handle}>
                    {scroll_content("view")}
                </ScrollView>
            </Group>
        </Group>
    }
}

/// Row 5: a virtualized 100k-row viewport, manually driven (see Page).
/// Only the visible window (+ overscan) is ever mounted; the leading row
/// index must advance continuously with no gaps or repeats.
#[component]
fn VirtualScrollRow(label: &'static str, handle: ScrollHandle) -> impl IntoElement {
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
                radius={12.0}
                fill={SURFACE}
                stroke={Stroke::inside(1.0, hairline())}
                clip={true}
            >
                <VirtualScrollView
                    width={Size::fill()}
                    height={Size::fill()}
                    len={100_000usize}
                    row_height={36.0}
                    handle={handle}
                    row={vrow}
                />
            </Group>
        </Group>
    }
}

/// One virtualized row: uniform 36px, index-addressed so gaps show.
fn vrow(i: usize) -> Element {
    group()
        .width(Size::fill())
        .height(36.0)
        .fill(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC })
        .children([
            text(format!("vrow {i:05} — pack my box with five dozen liquor jugs"))
                .size(14.0)
                .color(INK),
        ])
        .into_element()
}

/// Tall striped content with numbered rows so motion and holes are visible.
fn scroll_content(axis: &str) -> Element {
    let rows: Vec<Element> = (0..24)
        .map(|i| {
            group()
                .width(1200.0)
                .height(48.0)
                .fill(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC })
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
