// Responsibility: Scroll-demo page chrome — the design-system subset this demo
// needs (palette, labels, shell). Every ink is authored here from
// `rio-de-shader` stages: core carries selection geometry and thumb gestures,
// the fill logic sits one layer up, and the app supplies the looks.

use riode::internal::*;
use riode::prelude::*;
use riode_shader::Fill;

/// Tinted application background.
pub const PAGE: u32 = 0xF4F5F7;
/// Card surface.
pub const SURFACE: u32 = 0xFFFFFF;
/// Heading ink.
pub const INK: u32 = 0x0A0A0A;
/// Body copy grey.
pub const BODY: u32 = 0x7B7B7B;
/// Secondary label grey.
pub const MUTED: u32 = 0x999999;
/// Hairline source ink (always used with reduced alpha).
pub const HAIRLINE: u32 = 0x08090C;

/// One-pixel separator in the shared hairline colour.
pub fn hairline() -> Element {
    group()
        .width("fill")
        .height(1.0)
        .material(Fill::solid(Color::hex_alpha(HAIRLINE, 0.06)))
        .into_element()
}

/// Selection highlight ink, authored where fills live: classic selection blue
/// at 30%, painted one material layer per selected line, behind the glyphs.
pub fn selection_ink() -> Material {
    Fill::solid(Color::hex_alpha(0x0078D6, 0.30)).into()
}

/// Scrollbar thumb ink: the nameplate `THUMB_ALPHA` (0.5) over near-black.
pub fn thumb_ink() -> Material {
    Fill::solid(Color::hex_alpha(0x0A0A0A, 0.5)).into()
}

/// Uppercase eyebrow label; pass an already-uppercased literal.
#[component]
pub fn Eyebrow(label: &'static str) -> impl IntoElement {
    view! {
        <Text size={11.0} weight={700u16} color={MUTED}>{label}</Text>
    }
}

/// Page heading. Selectable: headings carry no handlers, so selecting never
/// swallows a click.
#[component]
pub fn Heading(title: &'static str) -> impl IntoElement {
    view! {
        <Text
            size={24.0}
            weight={700u16}
            color={INK}
            selectable={true}
            selection_fill={selection_ink()}
        >{title}</Text>
    }
}

/// Muted body copy wrapping inside its container.
#[component]
pub fn Blurb(copy: &'static str) -> impl IntoElement {
    view! {
        <Text
            size={13.0}
            line_height={20.0}
            color={BODY}
            width="fill"
            selectable={true}
            selection_fill={selection_ink()}
        >{copy}</Text>
    }
}

/// Height-filling content area beneath the header; clipped so nothing paints
/// past it.
#[component]
pub fn Content(children: Vec<Element>) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .clip(true)
        .margin(24.0)
        .children(children)
}

/// Page shell: header block, hairline, then the bounded content area.
#[component]
pub fn Shell(
    eyebrow_label: &'static str,
    title: &'static str,
    copy: &'static str,
    children: Vec<Element>,
) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .material(Fill::solid(PAGE))
        .children((
            group()
                .direction("vertical")
                .width("fill")
                .gap(8.0)
                .margin(Margin::sides(28.0, 28.0, 20.0, 28.0))
                .children((Eyebrow(eyebrow_label), Heading(title), Blurb(copy))),
            hairline(),
            Content(children),
        ))
}
