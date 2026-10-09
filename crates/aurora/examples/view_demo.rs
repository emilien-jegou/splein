// Single responsibility: view! macro demo — the same card in builder vs RSX form.
//
// Two rows render the same thing: row 1 hand-built with builders, row 2 built
// with `view!` through a typed-props #[component]. Human judges identity:
// same layout, same drift, same pixels. No test asserts.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 620;
const TRAVEL: f32 = 120.0;
const SWEEP_HZ: f64 = 0.5;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — view! demo")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(PAGE)
        .run(PageEntry);
}

/// The user's sketch: a pure function of time, no driver, no engine handle.
#[component]
pub fn PageEntry() -> impl IntoElement {
    let time = use_frame_clock();
    let travel = time.map(|t| ((t as f32) * SWEEP_HZ as f32).sin() * TRAVEL);

    view! {
        <Page
            eyebrow_label="MOTION · 01 — IDENTITY"
            title="Layout runs once. Transform runs every frame."
            copy="The card holds a fixed 280-wide box at a fixed origin. Only its compositor transform moves."
        >
            <Card travel={travel.clone()} label="view! row" />
            <Card travel={travel} label="view! row (again)" />
        </Page>
    }
}

#[component]
pub fn Page(
    eyebrow_label: Prop<String>,
    title: Prop<String>,
    copy: String,
    children: Vec<Element>,
) -> impl IntoElement {
    // Flat-forwarded children: an RSX `{children}` splice would interpose a
    // wrapper group (see motion_ui::Arena), so the vec is concatenated here.
    let mut kids = Vec::with_capacity(children.len() + 3);
    kids.push(text(eyebrow_label).size(11.0).color(MUTED).into_element());
    kids.push(text(title).size(20.0).color(INK).into_element());
    kids.push(text(copy).size(13.0).color(BODY).into_element());
    kids.extend(children);
    group()
        .direction("vertical")
        .width("fill")
        .height("fill")
        .gap(16.0)
        .children(kids)
}

#[component]
pub fn Card(travel: Prop<f32>, label: Prop<String>) -> impl IntoElement {
    view! {
        <Group
            width={280.0}
            height={96.0}
            x={travel}
            fill="#00FFFF"
            radius={16.0}
            stroke="1 #08090C inside"
            direction="vertical"
            shadow="0 4 16 #0006"
        >
            <Text size={14.0} color="ink">{label}</Text>
        </Group>
    }
}
