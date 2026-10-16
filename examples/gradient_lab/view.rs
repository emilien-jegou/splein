// Single responsibility: Linear-gradient + dithering lab (wgpu Bundle 2).
//
// Isolates plan_wgpu.md §3 (2-stop Oklab gradients) and §5-dither
// (Bayer 4x4 banding break) for human observation. Rows, one variable:
//   1. H gradient — blue to red, left to right.
//   2. Diagonal gradient — same stops, start (0,0) to end (360,64).
//   3. Alpha ramp — accent 40% to 100% (alpha interpolates with color).
//   4. Stop positions .25/.75 — transition compressed to the middle half.
//   5. Gradient + 3px border — border rides a second vertex over the fill.
//   6. Banding ramp — near-white warm/cool sweep where 8-bit banding
//      would show without the ordered dither.
//   7. Radial — expected MISSING on wgpu (documented fallback to vello).
// Cycle engines with F7; the human judges, no test asserts.

use super::motion_ui::*;
use riode::foundation::{ColorStop, LinearGradient, Point};
use riode::internal::*; // P10 escape hatch: this file composes repr-builders directly
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Gradient lab (wgpu bundle 2)";
pub const HOT_WIDTH: u32 = 860;
pub const HOT_HEIGHT: u32 = 780;
pub const HOT_BG: u32 = PAGE;

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Shell
            eyebrow_label="WGPU · GRADIENTS + DITHER"
            title="Gradient lab."
            copy="Two-stop linear gradients interpolate in Oklab with a 1/255 Bayer dither. Cycle engines (F7): color ramps should match tiny-skia/vello closely; the banding ramp must stay smooth on wgpu (dither); the radial card is expected missing on wgpu (vello fallback)."
        >
            <Content>
                <Group direction="vertical" width="fill" height="fill" gap={12.0}>
                    {row(
                        "H · B→R",
                        grad_card(linear(0.0, 0.0, 400.0, 0.0,
                            Color::hex(0x2F6FEB), Color::hex(0xE5484D), 0.0, 1.0)),
                    )}
                    {row(
                        "DIAG",
                        grad_card(linear(0.0, 0.0, 360.0, 64.0,
                            Color::hex(0x2F6FEB), Color::hex(0xE5484D), 0.0, 1.0)),
                    )}
                    {row(
                        "ALPHA 40→100",
                        grad_card(linear(0.0, 0.0, 400.0, 0.0,
                            Color::hex_alpha(ACCENT, 0.4), Color::hex_alpha(ACCENT, 1.0), 0.0, 1.0)),
                    )}
                    {row(
                        "STOPS .25/.75",
                        grad_card(linear(0.0, 0.0, 400.0, 0.0,
                            Color::hex(0x2F6FEB), Color::hex(0xE5484D), 0.25, 0.75)),
                    )}
                    {row(
                        "GRAD + BORDER",
                        group()
                            .width(400.0)
                            .height("fill")
                            .radius(12.0)
                            .fill(linear(0.0, 0.0, 400.0, 0.0,
                                Color::hex(0x2F6FEB), Color::hex(0xE5484D), 0.0, 1.0))
                            .stroke(Stroke::inside(3.0, Color::hex(INK))),
                    )}
                    {row_tall(
                        "BANDING",
                        grad_card_tall(linear(0.0, 0.0, 0.0, 96.0,
                            Color::hex(0xF7F4F1), Color::hex(0xE9EEF5), 0.0, 1.0)),
                    )}
                    {row(
                        "RADIAL · SKIP",
                        radial_card(),
                    )}
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

/// Extra-height labelled row for the banding ramp.
fn row_tall(label: &'static str, child: impl IntoElement) -> GroupDef {
    group()
        .direction("horizontal")
        .width("fill")
        .height(104.0)
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

/// Two-stop linear gradient in card-local coordinates. Stop positions
/// ride the gradient line from `start` to `end` (CSS parameterization).
fn linear(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    c0: Color,
    c1: Color,
    p0: f32,
    p1: f32,
) -> Fill {
    Fill::LinearGradient(LinearGradient {
        start: Point::new(x0, y0),
        end: Point::new(x1, y1),
        stops: vec![
            ColorStop {
                position: p0,
                color: c0,
            },
            ColorStop {
                position: p1,
                color: c1,
            },
        ],
    })
}

/// Fixed-width card carrying a gradient fill (400x64 matches the axis
/// coordinates above).
fn grad_card(fill: Fill) -> GroupDef {
    group()
        .width(400.0)
        .height("fill")
        .radius(12.0)
        .fill(fill)
}

fn grad_card_tall(fill: Fill) -> GroupDef {
    group()
        .width("fill")
        .height("fill")
        .radius(12.0)
        .fill(fill)
}

/// Radial gradient: the wgpu engine deliberately skips it (warn-once);
/// vello/tiny-skia render it. Present here as the documented fallback.
fn radial_card() -> GroupDef {
    group()
        .width(400.0)
        .height("fill")
        .radius(12.0)
        .fill(Fill::RadialGradient(riode::foundation::RadialGradient {
            center: Point::new(200.0, 32.0),
            radius: 220.0,
            stops: vec![
                ColorStop {
                    position: 0.0,
                    color: Color::hex(0x2F6FEB),
                },
                ColorStop {
                    position: 1.0,
                    color: Color::hex(0xE5484D),
                },
            ],
        }))
}
