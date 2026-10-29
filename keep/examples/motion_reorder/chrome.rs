// Single responsibility: Vercel-dark page chrome for this demo only.
//
// The shared `motion_ui` system is light-themed and used by every motion
// demo, so it stays untouched: this module mirrors the Shell/Panel/Stat
// shapes in Vercel black-and-white (page `#000`, cards `#0A0A0A`, hairline
// white-at-12%, white text, `#A1A1A1` secondary).

use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this file composes repr-builders directly

use super::super::motion_ui::{Eyebrow, EyebrowProps};

/// Vercel page background.
pub const VERCEL_BG: u32 = 0x000000;
/// Vercel card surface.
pub const VERCEL_SURFACE: u32 = 0x0A0A0A;
/// Vercel secondary text.
pub const VERCEL_GRAY: u32 = 0xA1A1A1;
/// Vercel row pill.
pub const VERCEL_PILL: u32 = 0x171717;

/// Hairline border for dark surfaces.
pub fn hairline_dark() -> Color {
    Color::white_alpha(0.12)
}

/// Page heading in white.
#[component]
pub fn WhiteHeading(title: &'static str) -> impl IntoElement {
    view! {
        <Text size={24.0} weight={700u16} color={Color::WHITE} selectable={true}>{title}</Text>
    }
}

/// Muted body copy in Vercel gray.
#[component]
pub fn GrayBlurb(copy: &'static str) -> impl IntoElement {
    view! {
        <Text size={13.0} line_height={20.0} color={VERCEL_GRAY} width="fill" selectable={true}>{copy}</Text>
    }
}

/// Label and value stacked, Vercel style: gray label, white value.
#[component]
pub fn VercelStat(label: &'static str, value: &'static str) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={2.0}>
            <Eyebrow label={label} />
            <Text size={13.0} weight={500u16} color={Color::WHITE}>{value}</Text>
        </Group>
    }
}

/// Fixed-height card header holding two elements apart.
#[component]
pub fn VercelStrip(left: Element, right: Element) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={52.0}
            alignment="center"
            margin={Margin::x(20.0)}
        >
            {left}
            <Group width="fill" />
            {right}
        </Group>
    }
}

/// Dark card holding a labelled header, a hairline and the list body.
///
/// Built with the builder API (not an RSX `{children}` splice): splicing a
/// `Vec<Element>` interposes an anonymous wrapper group and shifts layout.
#[component]
pub fn VercelPanel(head: Element, children: Vec<Element>) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .radius(16.0)
        .fill(VERCEL_SURFACE)
        .stroke(Stroke::inside(1.0, hairline_dark()))
        .children((
            head,
            group().width("fill").height(1.0).fill(hairline_dark()),
            group()
                .direction("vertical")
                .width("fill")
                .height("fill")
                .margin(20.0)
                .children(children),
        ))
}

/// Page shell: dark header block, hairline, then the bounded content area.
#[component]
pub fn VercelShell(
    eyebrow_label: &'static str,
    title: &'static str,
    copy: &'static str,
    children: Vec<Element>,
) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .fill(VERCEL_BG)
        .children((
            group()
                .direction("vertical")
                .width("fill")
                .gap(8.0)
                .margin(Margin::sides(28.0, 28.0, 20.0, 28.0))
                .children((
                    Eyebrow(eyebrow_label),
                    WhiteHeading(title),
                    GrayBlurb(copy),
                )),
            group().width("fill").height(1.0).fill(hairline_dark()),
            group()
                .direction("vertical")
                .width("fill")
                .height("fill")
                .clip(true)
                .margin(24.0)
                .children(children),
        ))
}
