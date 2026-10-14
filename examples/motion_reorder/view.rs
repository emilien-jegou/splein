// Single responsibility: Showcase dragging a list item into a new position.
// Module root: window chrome, demo constants, and the `Page` component.
// Gesture state lives in `drag`, the list container in `list`, one row in
// `row`, the release glide in `glide`.

#[path = "./chrome.rs"]
mod chrome;
#[path = "./drag.rs"]
mod drag;
#[path = "./glide.rs"]
mod glide;
#[path = "./list.rs"]
mod list;
#[path = "./row.rs"]
mod row;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::motion_ui::*;
use aurora::dsl::use_ref;
use aurora::prelude::*;

use chrome::{
    VercelPanelProps, VercelShellProps, VercelStatProps, VercelStripProps, VERCEL_BG,
};
use drag::Drag;
use list::list;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Web 04 · Drag to reorder";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 660;
pub const HOT_BG: u32 = VERCEL_BG;

/// Vertical padding inside each row, in pixels.
///
/// The one geometric constant in the demo, and a declaration rather than a measurement on
/// purpose: it is an *input* to layout. What the drag needs is the resulting distance between
/// rows, and that is measured from the rows themselves every step — see [`Drag::target_slot`].
///
/// Each row is an outer transparent group (the hit box, the measured slot, the animated
/// node) holding the visible accent row. Symmetric padding makes the spacing between
/// rows belong to the rows themselves, so presses starting there still grab a row.
/// Content keeps its exact height; slot centres drift by at most a few pixels versus
/// a gap, which measurement absorbs.
///
/// Nothing declares a row height. The rows fill the list and layout divides the space it was
/// actually given, so there is no height to fall out of step with the window.
pub const PAD: f32 = 4.0;
/// Settle spring for displaced (non-held) rows.
///
/// Local to the example so it can be tuned without touching the shared presets: overdamped,
/// so a row glides toward its new slot instead of overshooting through its neighbour.
pub const SETTLE: Spring = Spring::new(300.0, 38.0, 1.0);
/// Rows offered for reordering.
pub const LABELS: [&str; 5] = [
    "Baseline",
    "Underline",
    "Strikethrough",
    "Highlight",
    "Squiggle",
];

/// A draggable list that reorders itself from reactive state.
///
/// Three concerns stay separated, which is the point of the demo. The *list* is a signal;
/// reordering it re-reconciles the children by key. The *held row* follows the pointer. The
/// *other rows* travel from where they were to where layout now puts them, and that last part is
/// done here rather than by the engine.
///
/// The FLIP used to be an engine feature, on the claim that only layout knows where a row landed.
/// It does know, and it will say — but only if asked. Each row holds a handle to its own box,
/// [`use_flip`] does the four steps from measured positions, and [`use_follow`] keeps the held
/// row under the pointer from the same geometry. Nothing here computes a pitch, because there is
/// no single pitch to compute: a squashed window leaves the rows unequal.
#[component]
pub fn Page() -> impl IntoElement {
    let order = use_signal(LABELS.map(str::to_string).to_vec());
    // Bumped on every reorder, which is what tells each row's FLIP that the arrangement changed.
    //
    // A revision rather than the order itself: comparing five labels per row per frame is work
    // proportional to the list, and the only question is "is this a different arrangement from
    // the one whose positions I cached".
    let revision = use_signal(0u32);
    // Which row is held, by identity rather than by slot. A reorder changes a row's index, so
    // tracking the index would drop the hold the moment the list shuffled underneath it.
    let held = use_signal(None::<String>);
    // Where in the held row the pointer took hold, so the row keeps that point under the cursor
    // instead of snapping its centre to it.
    let grab_dy = use_signal(0.0f32);
    // Where the pointer is asking the held row's centre to be, in window coordinates.
    let want_centre = use_signal(None::<f32>);

    let drag = Rc::new(Drag {
        order: order.clone(),
        revision: revision.clone(),
        held: held.clone(),
        grab_dy: grab_dy.clone(),
        want_centre: want_centre.clone(),
        // One handle for the list, which is how the gesture learns where the rows are.
        list: use_ref(),
        settlers: RefCell::new(HashMap::new()),
    });

    view! {
        <VercelShell
            eyebrow_label="WEB · 04 — DRAG TO REORDER"
            title="Grab a row, drop it anywhere in the list."
            copy="While held, the row follows the pointer as a compositor transform, so layout never runs. Crossing a row swaps the list once, and every other row springs from where it was to where layout now puts it. All of it is measured, so squashing the window mid-drag stays exact."
        >
            <Content>
                <VercelPanel
                    head={<VercelStrip
                        left={<VercelStat label="DRAG" value="compositor transform" />}
                        right={<VercelStat label="SWAP" value="measured FLIP" />}
                    />}
                >
                    {list(drag)}
                </VercelPanel>
            </Content>
        </VercelShell>
    }
}
