// Single responsibility: An accordion item's revealed copy is uncovered by its growing box.
//
// The reveal is two mechanisms working together: a spring moves the box, and a clip cuts the
// content at the box's edge. Either alone looks wrong -- a box that grows through empty space
// leaves the copy fully visible for the whole second half of the animation, and content that is
// never clipped just sits there while the box moves around it.

#[allow(dead_code)]
#[path = "../examples/motion_accordion/main.rs"]
mod accordion;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::foundation::Color;
use aurora::reactive::{Cx, ReactiveRuntime, Signal};
use aurora_tiny_skia::testing::{Harness, Rgba};

/// The dark arena behind the cards, which must never be mistaken for content.
const ARENA: Rgba = Rgba {
    r: 0x0A,
    g: 0x0A,
    b: 0x0A,
    a: 255,
};

fn open_at(index: usize) -> (Harness, Rc<RefCell<ReactiveRuntime>>) {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&rt), Some(index));
    let h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(|| accordion::Page(open.clone())),
        accordion::WIDTH,
        accordion::HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7));
    (h, rt)
}

/// Rows inside the card's horizontal band that carry the copy rather than a background.
///
/// Content is detected by *darkness*, not by matching a background colour. Matching white and
/// the dark arena exactly is not enough: antialiased card corners and the page showing through
/// the gutter land on every shade in between, and each one reads as a false positive at a
/// different threshold. The copy sits at luminance ~0.48 and every background -- white, the
/// page, the arena, and any blend of them -- is above 0.9, so one cutoff separates them.
///
/// A row also has to carry real ink density, since a single stray pixel is noise.
/// Target height of an open item, owned by the demo rather than restated here.
const OPEN: f32 = accordion::panels::OPEN;

const MIN_INK_PER_ROW: u32 = 6;
/// Copy is mid-grey; the lightest background blend in the gutter is near-white.
const CONTENT_MAX_LUMA: f32 = 0.70;

fn luma(p: aurora_tiny_skia::testing::Rgba) -> f32 {
    (p.r as f32 + p.g as f32 + p.b as f32) / 765.0
}

fn content_rows(h: &mut Harness, index: usize, from: f32, to: f32) -> Vec<u32> {
    let panel = h.located(&format!("panel-{index}")).absolute;
    let x0 = (panel.x + 26.0) as u32;
    let x1 = (panel.x + panel.width - 26.0) as u32;
    let shot = h.view();
    let mut rows = Vec::new();
    let start = (panel.y + from) as u32;
    let end = ((panel.y + to) as u32).min(shot.height());
    for y in start..end {
        let mut hits = 0;
        for x in x0..x1.min(shot.width()) {
            if let Some(p) = shot.at(x, y) {
                // The arena is dark but is background, so it is excluded by colour while the
                // copy is admitted by darkness.
                if luma(p) < CONTENT_MAX_LUMA && !p.near(ARENA, 30) {
                    hits += 1;
                }
            }
        }
        if hits >= MIN_INK_PER_ROW {
            rows.push(y);
        }
    }
    rows.dedup();
    rows
}

#[test]
#[ntest::timeout(5_000)]
fn nothing_is_painted_below_the_growing_box() {
    // The reveal is a clip, so anything the clip does not cut is a bug that reads as a card
    // leaking copy into the gutter between items.
    let (mut h, _rt) = open_at(0);
    h.steps(10);
    h.click("item-1");
    h.pause();

    // Ten single-quantum steps, so about 83ms into a spring that runs for several hundred. This
    // is the part of the reveal worth checking -- the box is small and the copy is genuinely
    // clipped -- and it is the expensive end of it: every step here rasterizes the whole surface,
    // because an animating height invalidates layout and layout invalidates the window. Sampling
    // the whole flight cost about thirty seconds of solid rasterization to look at pixels nobody
    // changed after the first few frames.
    for step in 0..10 {
        h.step_once();
        let panel = h.located("panel-1").absolute;
        // The gutter is bounded by the *next* item, and that item now moves: the height being
        // animated is a real layout value, so the sibling below is laid out at the live height
        // rather than at a settled one. Measuring a fixed band below this panel would therefore
        // run into the neighbour instead of into empty space.
        let next_top = h.located("panel-2").absolute.y;
        assert!(
            next_top >= panel.y + panel.height - 0.5,
            "step {step}: the next item starts at {next_top:.1} but this one ends at {:.1}",
            panel.y + panel.height
        );
        // Everything between this item's bottom edge and the next item's top is gutter, and the
        // gutter must stay empty: copy there reads as this card leaking into the gap.
        let escaped = content_rows(&mut h, 1, panel.height, next_top - panel.y);
        assert!(
            escaped.is_empty(),
            "step {step}: item is {:.1} tall and {escaped:?} were painted in the gutter below it",
            panel.height
        );
    }
}

#[test]
#[ntest::timeout(5_000)]
fn the_copy_is_revealed_progressively_rather_than_all_at_once() {
    // If the copy fits inside the smallest box the item ever takes, the clip never has to cut
    // it: it is simply fully visible for the entire animation while the box grows around it,
    // which is what "already expanded" looks like.
    let (mut h, _rt) = open_at(0);
    h.steps(20);
    h.click("item-1");
    h.pause();

    let mut revealed = Vec::new();
    for _ in 0..10 {
        h.step_once();
        let render = h.located("panel-1").render.height;
        revealed.push((render, content_rows(&mut h, 1, 0.0, render).len() as f32));
    }

    let first = revealed[0].1;
    let last = revealed[revealed.len() - 1].1;
    assert!(
        first < last * 0.6,
        "the copy should appear as the box grows, but {first:.0} rows were already visible at the \\
         first step against {last:.0} at the last: {revealed:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn the_copy_sits_centred_in_the_body_rather_than_above_a_gap() {
    // The original body was 120px tall and the copy filled about 40 of them, all of it at the
    // top: the box kept growing for another 80px after the text was fully visible, with the
    // rounded bottom corners floating below it. What matters is not that the gap vanished --
    // a one-line copy in a 120px body always has some -- but that it is not all at one end.
    for index in 0..4 {
        let (mut h, _rt) = open_at(index);
        h.steps(400);

        let panel = h.located(&format!("panel-{index}")).absolute;
        let rows = content_rows(&mut h, index, 0.0, panel.height);
        assert!(!rows.is_empty(), "item {index} shows no copy at all");

        let header = 56.0;
        let body = panel.height - header;
        // Rows inside the body only; the header's own title is ink too.
        let in_body: Vec<f32> = rows
            .iter()
            .map(|y| *y as f32 - panel.y - header)
            .filter(|y| *y > 4.0)
            .collect();
        let (lo, hi) = (
            in_body.iter().cloned().fold(f32::MAX, f32::min),
            in_body.iter().cloned().fold(f32::MIN, f32::max),
        );

        let above = lo;
        let below = body - hi;
        assert!(
            (above - below).abs() <= 26.0,
            "item {index}: {above:.0}px above the copy and {below:.0}px below it in a \
             {body:.0}px body: the slack is all at one end, so the box's travel animates nothing"
        );
        assert!(
            below < body * 0.6,
            "item {index}: {below:.0}px of the {body:.0}px body is below the copy"
        );
    }
}

#[test]
#[ntest::timeout(5_000)]
fn every_item_reveals_the_same_way() {
    // The reveal is a property of the item, so one item behaving differently is a bug even
    // though nothing about it looks special.
    let mut extents = Vec::new();
    for index in 0..4 {
        let (mut h, _rt) = open_at(index);
        h.steps(400);
        let panel = h.located(&format!("panel-{index}")).absolute;
        let rows = content_rows(&mut h, index, 0.0, panel.height);
        assert!(!rows.is_empty(), "item {index} revealed no copy at all");
        extents.push(panel.height);
    }
    // Exact again. This was loosened while the panels were being silently shrunk by the layout
    // pass, which made every open item settle a few pixels short of the height it declares. With
    // the demo sized so the open list fits, every item has its declared height, so the lockstep
    // assertion is the stronger one and is restored.
    for (index, height) in extents.iter().enumerate() {
        assert_eq!(
            *height, OPEN,
            "item {index} must settle at the height it declares"
        );
    }
}
