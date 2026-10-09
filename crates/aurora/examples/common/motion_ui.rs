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

/// Rust-callable twins of the leaf components, for `{...}` splices.
/// Prefer tags in RSX — including nested in attributes
/// (`head={<Strip left={<Stat …/>} />}`).
pub fn eyebrow(label: &'static str) -> Element {
    Eyebrow(label)
}

/// Rust-callable twin of [`Heading`].
pub fn heading(title: &'static str) -> Element {
    Heading(title)
}

/// Rust-callable twin of [`Blurb`].
pub fn blurb(copy: &'static str) -> Element {
    Blurb(copy)
}

/// White card with the shared border and radius, lifted by page contrast rather than shadow.
pub fn surface() -> GroupDef {
    group()
        .radius(16.0)
        .fill(SURFACE)
        .stroke(Stroke::inside(1.0, hairline()))
}

/// Uppercase eyebrow label; pass an already-uppercased literal.
#[component]
pub fn Eyebrow(label: &'static str) -> impl IntoElement {
    view! {
        <Text size={11.0} weight={700u16} color={MUTED}>{label}</Text>
    }
}

/// Page heading.
///
/// Selectable body content: headings carry no handlers, so selecting never
/// swallows a click.
#[component]
pub fn Heading(title: &'static str) -> impl IntoElement {
    view! {
        <Text size={24.0} weight={700u16} color={INK} selectable={true}>{title}</Text>
    }
}

/// Muted body copy wrapping inside its container.
#[component]
pub fn Blurb(copy: &'static str) -> impl IntoElement {
    view! {
        <Text size={13.0} line_height={20.0} color={BODY} width="fill" selectable={true}>{copy}</Text>
    }
}

/// Label and value stacked with the shared spacing rhythm.
#[component]
pub fn Stat(label: &'static str, value: &'static str) -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={2.0}>
            <Eyebrow label={label} />
            <Text size={13.0} weight={500u16} color={INK}>{value}</Text>
        </Group>
    }
}

/// Fixed-height card header holding two elements apart.
#[component]
pub fn Strip(left: Element, right: Element) -> impl IntoElement {
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

/// Dark inset arena that bounds a moving element's travel.
///
/// NOTE: bodies forward `children` through builder `.children()` (flat) rather
/// than an RSX `{children}` splice — the splice would route through
/// `Vec<Element>: IntoElement`, interposing an anonymous wrapper group and
/// shifting layout. RSX *nesting* at call sites stays free: `view!` hands the
/// vec straight to the props builder.
#[component]
pub fn Arena(children: Vec<Element>) -> impl IntoElement {
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
            .children(children)])
}

/// Dark status chip carrying a live label, in the floating-HUD idiom.
#[component]
pub fn StatusChip(children: Vec<Element>) -> impl IntoElement {
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
            .children(children)])
}

/// White card holding a labelled header, a hairline and a bounded arena.
#[component]
pub fn Panel(head: Element, children: Vec<Element>) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .radius(16.0)
        .fill(SURFACE)
        .stroke(Stroke::inside(1.0, hairline()))
        .children((
            head,
            group().width("fill").height(1.0).fill(hairline()),
            group()
                .width("fill")
                .height("fill")
                .margin(20.0)
                .children([Arena(children)]),
        ))
}

/// Height-filling content area beneath the header; clipped so nothing paints past it.
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
        .fill(PAGE)
        .children((
            group()
                .direction("vertical")
                .width("fill")
                .gap(8.0)
                .margin(Margin::sides(28.0, 28.0, 20.0, 28.0))
                .children((Eyebrow(eyebrow_label), Heading(title), Blurb(copy))),
            group().width("fill").height(1.0).fill(hairline()),
            Content(children),
        ))
}
