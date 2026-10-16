// Single responsibility: Per-edge border + linear-blending lab (wgpu Bundle 1).
//
// Isolates plan_wgpu.md §1 (per-edge borders) and §2 (linear-space
// blending) for human observation. Rows, one variable each:
//   1. per-edge inside widths (1/4/8/16) — wgpu shows four distinct edge
//      widths; engines without per-edge support render uniform-max (16).
//   2. alignment (inside / center / outside, 4px) — center straddles the
//      layout edge, outside extends past it.
//   3. radii (0 / 12 / 32) with a uniform 2px border.
//   4. translucent overlap — linear compositing (wgpu) vs encoded-space
//      (other engines): the overlap patch must differ.
//   5. text on an 85%-white card over accent — linear blending keeps
//      small-text edges from going dark/thin.
// Cycle engines with F7; the human judges, no test asserts.

use super::motion_ui::*;
use riode::internal::*; // P10 escape hatch: this file composes repr-builders directly
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Border lab (wgpu bundle 1)";
pub const HOT_WIDTH: u32 = 860;
pub const HOT_HEIGHT: u32 = 740;
pub const HOT_BG: u32 = PAGE;

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Shell
            eyebrow_label="WGPU · BORDERS + LINEAR BLEND"
            title="Border lab."
            copy="One row per variable: per-edge widths, alignment, radii, translucent overlap, text on translucency. Cycle engines (F7): the per-edge row shows four distinct widths only on wgpu; the overlap patch and text edges shift when compositing moves to linear."
        >
            <Content>
                <Group direction="vertical" width="fill" height="fill" gap={14.0}>
                    {row(
                        "PER-EDGE",
                        group()
                            .width("fill")
                            .height("fill")
                            .radius(12.0)
                            .fill(SURFACE)
                            .stroke(per_edge_stroke()),
                    )}
                    {row(
                        "ALIGN ×3",
                        group()
                            .direction("horizontal")
                            .width("fill")
                            .height("fill")
                            .gap(12.0)
                            .children([
                                align_card("INSIDE 4", Stroke::inside(4.0, Color::hex(ACCENT))),
                                align_card("CENTER 4", Stroke::center(4.0, Color::hex(ACCENT))),
                                align_card("OUTSIDE 4", Stroke::outside(4.0, Color::hex(ACCENT))),
                            ]),
                    )}
                    {row(
                        "RADII ×3",
                        group()
                            .direction("horizontal")
                            .width("fill")
                            .height("fill")
                            .gap(12.0)
                            .children([
                                radius_card("R0", 0.0),
                                radius_card("R12", 12.0),
                                radius_card("R32", 32.0),
                            ]),
                    )}
                    {row("HAIRLINE", hairline_card())}
                    {row_tall("OVERLAP 55%", overlap_cards())}
                    {row_tall("TEXT ON 85%", text_card())}
                </Group>
            </Content>
        </Shell>
    }
}

/// Standard-height labelled row.
fn row(label: &'static str, child: impl IntoElement) -> GroupDef {
    group()
        .direction("horizontal")
        .width("fill")
        .height(64.0)
        .alignment("center")
        .gap(14.0)
        .children([
            group()
                .width(110.0)
                .height("fill")
                .alignment("center")
                .children([Eyebrow(label)]),
            group().width("fill").height("fill").children([child]),
        ])
}

/// Extra-height labelled row for the overlap and text samples.
fn row_tall(label: &'static str, child: impl IntoElement) -> GroupDef {
    group()
        .direction("horizontal")
        .width("fill")
        .height(110.0)
        .alignment("center")
        .gap(14.0)
        .children([
            group()
                .width(110.0)
                .height("fill")
                .alignment("center")
                .children([Eyebrow(label)]),
            group().width("fill").height("fill").children([child]),
        ])
}

/// Inside border with four distinct per-edge widths: top 1, right 4,
/// bottom 8, left 16. On engines without per-edge support this renders
/// as a uniform 16px border (the stroke's `width` mirrors the widest
/// edge for graceful degradation).
fn per_edge_stroke() -> Stroke {
    Stroke::inside(0.0, Color::hex(ACCENT)).per_edge(Edges::new(1.0, 4.0, 8.0, 16.0))
}

/// Centred card label for the alignment and radius rows.
fn align_card(label: &'static str, stroke: Stroke) -> GroupDef {
    group()
        .width("fill")
        .height("fill")
        .radius(12.0)
        .fill(SURFACE)
        .stroke(stroke)
        .alignment("center")
        .children([Eyebrow(label)])
}

fn radius_card(label: &'static str, radius: f32) -> GroupDef {
    group()
        .width("fill")
        .height("fill")
        .radius(radius)
        .fill(SURFACE)
        .stroke(Stroke::inside(2.0, Color::hex(INK)))
        .alignment("center")
        .children([Eyebrow(label)])
}

/// Sub-half-pixel inside border: the AA fringe must stay even, no
/// stepping or dropped edge.
fn hairline_card() -> GroupDef {
    group()
        .width("fill")
        .height("fill")
        .radius(12.0)
        .fill(SURFACE)
        .stroke(Stroke::inside(0.5, Color::hex(HAIRLINE)))
}

/// Two 55%-alpha cards overlapping by 90px. The overlap patch is the
/// linear-vs-encoded discriminator: linear compositing (wgpu) yields a
/// lighter, less saturated patch than sRGB-space compositing.
fn overlap_cards() -> GroupDef {
    group()
        .direction("horizontal")
        .width("fill")
        .height("fill")
        .alignment("center")
        .gap(0.0)
        .children([
            group()
                .width(260.0)
                .height(72.0)
                .radius(12.0)
                .fill(Color::hex_alpha(ACCENT, 0.55)),
            group()
                .width(260.0)
                .height(72.0)
                .radius(12.0)
                .fill(Color::hex_alpha(0xE5484D, 0.55))
                .margin(Margin::left(-90.0)),
        ])
}

/// Small (13px) and display (48px) text on an 85%-white card over the
/// accent band: linear blending must keep the small text from going
/// dark or thin at the translucent boundary.
fn text_card() -> GroupDef {
    group()
        .width("fill")
        .height("fill")
        .radius(12.0)
        .fill(Color::hex(ACCENT))
        .alignment("center")
        .children([group()
            .direction("vertical")
            .width("fill")
            .height("fill")
            .radius(12.0)
            .fill(Color::white_alpha(0.85))
            .alignment("center")
            .gap(4.0)
            .children([
                text("The quick brown fox jumps over the lazy dog")
                    .size(13.0)
                    .weight(500)
                    .color(INK),
                text("Ag · 48px").size(48.0).weight(700).color(INK),
            ])])
}
