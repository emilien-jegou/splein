// Responsibility: Scroll-paint probe — one variable per row, bisecting why
// scroll_demo shows a blank page. Each row is a saturated colored strip (so
// MATERIAL paint is readable even when GLYPHS fail) with dark text on top.
// One run of this page localizes the failing construct.
//
//   P1  bare text on the window background — no group, no material
//   P2  text inside a material group (demo1 idiom)
//   P3  text inside a material group inside a clipped group
//   P4  text through the chrome component path (u32 color, weight 700)
//   P5  chrome Blurb path: material root + tuple children + clip + margin
//   P6  SCROLL-channel drift: scroll_offset driven by the frame clock
//   P7  ScrollView mount with thumb_fill (the restored thumb)

use riode::internal::*;
use riode::prelude::*;
use riode_shader::Fill;

pub const HOT_TITLE: &str = "Aurora — scroll paint probe";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 700;
pub const HOT_BG: u32 = 0xFFFFFF;

const INK: u32 = 0x0A0A0A;

/// One probe row: saturated strip + a label prefix + the construct under test.
fn strip(color: u32, label: &'static str, child: Element) -> Element {
    group()
        .direction("horizontal")
        .width("fill")
        .height(80.0)
        .material(Fill::solid(color))
        .children([
            text(format!("{label}:"))
                .size(18.0)
                .color(0xFFFFFF)
                .width(60.0)
                .into_element(),
            child,
        ])
        .into_element()
}

fn plain_text(label: &'static str) -> Element {
    text(format!("{label} — the quick brown fox jumps over the lazy dog"))
        .size(18.0)
        .color(INK)
        .into_element()
}

#[component(name = "scroll_probe::Page")]
pub fn Page() -> impl IntoElement {
    // P6 driver: slow left-right drift on the SCROLL channel (clock is f64).
    let clock = use_frame_clock();
    let x = clock.map(|secs| {
        let phase = (secs % 16.0) / 16.0;
        let tri = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
        tri as f32 * 120.0
    });

    // P7 driver: uncontrolled scroll; human drags/wheels it.
    let _sc = use_scroll(0.0);
    let rows: Vec<Element> = vec![
        // P5: tuple children + margins, Shell-style.
        strip(0x8E24AA, "P5", {
            group()
                .width("fill")
                .height("fill")
                .children((
                    plain_text("tuple child A"),
                    plain_text("tuple child B"),
                ))
                .into_element()
        }),
        // P6: SCROLL-channel drift (frame clock → scroll_offset).
        strip(0x00ACC1, "P6", {
            group()
                .width("fill")
                .height("fill")
                .clip(true)
                .children([group()
                    .absolute()
                    .children([plain_text("drifting scroll text — moves slowly left/right")])
                    .scroll_offset(x)
                    .into_element()])
                .into_element()
        }),
    ];

    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .children(rows)
        .into_element()
}

/// P7 content: a real `ScrollView` (striped text rows), uncontrolled, with the
/// restored `thumb_fill` ink — drag or wheel to bring the dark thumb up.
fn scroll_view_probe() -> Element {
    let thumb: Material = Fill::solid(Color::hex_alpha(0x0A0A0A, 0.5)).into();
    view! {
        <ScrollView
            width={Size::fill()}
            height={Size::fill()}
            thumb_fill={thumb}
        >
            {scroll_rows()}
        </ScrollView>
    }
    .into_element()
}

fn scroll_rows() -> Vec<Element> {
    (0..12)
        .map(|i| {
            group()
                .width(Size::fill())
                .height(40.0)
                .material(Fill::solid(if i % 2 == 0 { 0xF5F5F5 } else { 0xE0E0E0 }))
                .children([text(format!("scroll row {i:02} — pack my box with five dozen liquor jugs"))
                    .size(16.0)
                    .color(INK)])
                .into_element()
        })
        .collect()
}
