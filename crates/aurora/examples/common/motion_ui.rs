// Single responsibility: Shared light design system and page shells for the motion demos.
#![allow(dead_code)]

use aurora::prelude::*;

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
/// Accent used for motion indicators.
pub const ACCENT: u32 = 0x1019EC;
/// Hairline source ink.
pub const HAIRLINE: u32 = 0x08090C;
/// Dark chip and arena surface.
pub const CHIP: u32 = 0x0A0A0A;

/// One-pixel separator in the shared hairline colour.
pub fn hairline() -> Color {
    Color::hex_alpha(HAIRLINE, 0.06)
}

/// White card with the shared border and radius, lifted by page contrast rather than shadow.
pub fn surface() -> GroupDef {
    group()
        .radius(16.0)
        .fill(SURFACE)
        .stroke(Stroke::inside(1.0, hairline()))
}

/// Uppercase eyebrow label; pass an already-uppercased literal.
pub fn eyebrow(label: &'static str) -> TextDef {
    text(label).size(11.0).weight(700).color(MUTED)
}

/// Page heading.
///
/// Selectable body content: headings carry no handlers, so selecting never
/// swallows a click.
pub fn heading(title: &'static str) -> TextDef {
    text(title)
        .size(24.0)
        .weight(700)
        .color(INK)
        .selectable(true)
}

/// Muted body copy wrapping inside its container.
pub fn blurb(copy: &'static str) -> TextDef {
    text(copy)
        .size(13.0)
        .line_height(20.0)
        .color(BODY)
        .width("fill")
        .selectable(true)
}

/// Label and value stacked with the shared spacing rhythm.
pub fn stat(label: &'static str, value: &'static str) -> GroupDef {
    group().direction("vertical").gap(2.0).children((
        eyebrow(label),
        text(value).size(13.0).weight(500).color(INK),
    ))
}

/// Fixed-height card header holding two elements apart.
pub fn strip(left: impl IntoElement, right: impl IntoElement) -> GroupDef {
    group()
        .direction("horizontal")
        .width("fill")
        .height(52.0)
        .alignment("center")
        .margin(Margin::x(20.0))
        .children((left, group().width("fill"), right))
}

/// Dark inset arena that bounds a moving element's travel.
pub fn arena(content: impl IntoElement) -> GroupDef {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .radius(12.0)
        .clip(true)
        .fill(CHIP)
        .stroke(Stroke::inside(1.0, Color::white_alpha(0.08)))
        .children([group()
            .direction("vertical")
            .width("fill")
            .height("fill")
            .margin(16.0)
            .alignment("center")
            .children([content])])
}

/// Dark status chip carrying a live label, in the floating-HUD idiom.
pub fn status_chip(label: impl IntoElement) -> GroupDef {
    group()
        .direction("horizontal")
        .width("fit")
        .height(26.0)
        .radius("max")
        .fill(CHIP)
        .stroke(Stroke::inside(1.0, Color::white_alpha(0.15)))
        .alignment("center")
        .children([group()
            .height("fill")
            .alignment("center")
            .margin(Margin::x(12.0))
            .children([label])])
}

/// White card holding a labelled header, a hairline and a bounded arena.
pub fn panel(head: impl IntoElement, body: impl IntoElement) -> GroupDef {
    surface()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .children((
            head,
            group().width("fill").height(1.0).fill(hairline()),
            group()
                .width("fill")
                .height("fill")
                .margin(20.0)
                .children([arena(body)]),
        ))
}

/// Height-filling content area beneath the header; clipped so nothing paints past it.
pub fn content(body: impl IntoElement) -> GroupDef {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .clip(true)
        .margin(24.0)
        .children([body])
}

/// Page shell: header block, hairline, then the bounded content area.
pub fn page(
    eyebrow_label: &'static str,
    title: &'static str,
    copy: &'static str,
    body: impl IntoElement,
) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .fill(PAGE)
        .children((
            group()
                .direction("vertical")
                .width("fill")
                .gap(8.0)
                .margin(Margin::sides(28.0, 28.0, 20.0, 28.0))
                .children((eyebrow(eyebrow_label), heading(title), blurb(copy))),
            group().width("fill").height(1.0).fill(hairline()),
            content(body),
        ))
}
