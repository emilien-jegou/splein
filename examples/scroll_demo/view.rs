// Responsibility: Scroll-demo page assembly and window chrome — the clock
// drivers, the auto-scroll ticker, and the row layout. Restoration targets:
// row 4 shows the thumb paint, the header + selection card show text selection.

use riode::prelude::*;

use super::chrome::{ShellProps, PAGE};
use super::rows::{
    drift_rows, ScrollViewRowProps, SelectionRowProps, TRAVEL, VirtualScrollRowProps,
};

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Scroll · thumb · selection";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 985;
pub const HOT_BG: u32 = PAGE;

/// Three SCROLL-channel drift runs, one auto-scrolling `ScrollView`, one
/// manual 100k-row `VirtualScrollView`, and a selectable-text card.
#[component(name = "scroll_demo::Page")]
pub fn Page() -> impl IntoElement {
    // Frame-clock ping-pong: 16s period (8s out, 8s back) => ~20px/s, subpixel
    // per frame.
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
    // range. Continuous motion keeps the thumb on — exactly what the
    // `thumb_fill` paint has to hold together with the fade pump.
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
    // auto-scroll would fight wheel/drag/key input).
    let vsc = use_scroll(0.0);

    view! {
        <Shell
            eyebrow_label="SCROLL · THUMB · SELECTION"
            title="Scroll channel drift, scrollbar thumb paint, live text selection."
            copy="Rows 1-3 drift at ~20px/s. Row 4 auto-scrolls a 24-row ScrollView (thumb fill top-right); row 5 is a manually-driven 100k-row VirtualScrollView (wheel, drag, thumb, keys). The SELECTION card is selectable text: drag for the band, double-click for a word, Ctrl+C copies."
        >
            <Group direction="vertical" width="fill" height="fill" gap={16.0}>
                {drift_rows(x, y, xy)}
                <ScrollViewRow label="SCROLLVIEW" handle={sc} />
                <VirtualScrollRow label="VIRTUAL 100K" handle={vsc} />
                <SelectionRow label="SELECTION" />
            </Group>
        </Shell>
    }
}
