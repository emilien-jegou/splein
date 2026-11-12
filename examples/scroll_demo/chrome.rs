// Responsibility: Scroll-demo design-system chrome, color palette, labels, and outer shell framing.

use riode::{dsl::GroupElement, prelude::*};
use riode_shader::Fill;

pub const PAGE: u32 = 0xF4F5F7;
pub const SURFACE: u32 = 0xFFFFFF;
pub const INK: u32 = 0x0A0A0A;
pub const BODY: u32 = 0x7B7B7B;
pub const MUTED: u32 = 0x999999;
pub const HAIRLINE: u32 = 0x08090C;

pub fn hairline() -> GroupElement {
    view! {
        <g w="fill" h=1 material={Fill::solid(Color::hex_alpha(HAIRLINE, 0.06))} />
    }
}

pub fn selection_ink() -> Material {
    Fill::solid(Color::hex_alpha(0x0078D6, 0.30)).into()
}

pub fn thumb_ink() -> Material {
    Fill::solid(Color::hex_alpha(0x0A0A0A, 0.5)).into()
}

#[component]
pub fn Eyebrow(label: &'static str) -> impl IntoElement {
    view! {
        <text s=11 w=700 color={MUTED}>{label}</text>
    }
}

#[component]
pub fn Heading(title: &'static str) -> impl IntoElement {
    view! {
        <text
            s=24
            w=700
            color={INK}
            selectable
            selection_fill={selection_ink()}
        >{title}</text>
    }
}

#[component]
pub fn Blurb(copy: &'static str) -> impl IntoElement {
    view! {
        <text
            s=13
            line_height=20.0
            color={BODY}
            width="fill"
            selectable
            selection_fill={selection_ink()}
        >{copy}</text>
    }
}

#[component]
pub fn Content(children: Vec<Element>) -> impl IntoElement {
    view! {
        <g col w="fill" h="fill" clip m=24>
            {children}
        </g>
    }
}

#[component]
pub fn Shell(
    eyebrow_label: &'static str,
    title: &'static str,
    copy: &'static str,
    children: Vec<Element>,
) -> impl IntoElement {
    view! {
        <g col w="fill" h="fill" material={Fill::solid(PAGE)}>
            <g col w="fill" gap=8 mx=28 mt=28 mb=20>
                <Eyebrow label={eyebrow_label} />
                <Heading title={title} />
                <Blurb copy={copy} />
            </g>
            {hairline()}
            <Content>{children}</Content>
        </g>
    }
}
