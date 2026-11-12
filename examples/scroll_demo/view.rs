// Responsibility: Scroll-demo page assembly, clock drivers, auto-scroll interval, and root shell layout.

use riode::prelude::*;

use super::chrome::*;
use super::rows::*;

pub const HOT_TITLE: &str = "Aurora — Scroll · thumb · selection";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 985;
pub const HOT_BG: u32 = PAGE;

#[component(name = "scroll_demo::Page")]
pub fn Page() -> impl IntoElement {
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 16.0) / 16.0;
        let tri = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
        tri as f32
    });

    let x = t.clone().map(|v| v * TRAVEL);
    let y = t.clone().map(|v| v * TRAVEL);
    let xy = t.map(|v| Transform::from_translation(-v * TRAVEL, -v * TRAVEL));

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

    let vsc = use_scroll(0.0);

    view! {
        <Shell
            eyebrow_label="SCROLL · THUMB · SELECTION"
            title="Scroll channel drift, scrollbar thumb paint, live text selection."
            copy="Rows 1-3 drift at ~20px/s. Row 4 auto-scrolls a 24-row ScrollView (thumb fill top-right); row 5 is a manually-driven 100k-row VirtualScrollView (wheel, drag, thumb, keys). The SELECTION card is selectable text: drag for the band, double-click for a word, Ctrl+C copies."
        >
            <g col w="fill" h="fill" gap=16>
                <DriftRows x={x} y={y} xy={xy} />
                <ScrollViewRow label="SCROLLVIEW" handle={sc} />
                <VirtualScrollRow label="VIRTUAL 100K" handle={vsc} />
                <SelectionRow label="SELECTION" />
            </g>
        </Shell>
    }
}
