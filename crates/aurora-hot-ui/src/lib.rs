// Hot-reloadable demo UI: real components, re-executed in place.
//
// This crate builds as a `cdylib` that the component host loads. Edit anything here —
// colors, structure, hooks, branches — save, and the running window rebuilds from the
// fresh library with every signal, spring, and ticker intact. Rules for this file:
//
// - Resolve `aurora` to the shared instance (the `extern crate` alias below): linking
//   aurora statically would embed a second copy with disjoint hook scopes, and component
//   code would panic. Same toolchain both sides (Rust has no stable ABI).
// - Pin stable identities (`#[component(name = "...")]`) so state survives file moves.
// - Keep the exported symbols and their signatures exactly as below.
//
// The remote data-description half (`aurora_remote_*`, below the components) serves the
// data-only loop instead; both halves share this crate so each demo stays verifiable.

extern crate aurora_shared as aurora;

use aurora::dsl::IntoElement;
use aurora::hot_reload::remote::{
    RemoteDirection, RemoteNode, RemoteSize, RemoteSnapshot, RemoteStateDecl, RemoteText,
    StateWrite, REMOTE_ABI_VERSION,
};
use aurora::hot_reload::COMPONENT_ABI_VERSION;
use aurora::prelude::*;

/// Click counter: hook state proving in-place continuity across reloads.
#[component(name = "hot_ui::Counter")]
fn Counter() -> impl IntoElement {
    let taps = use_signal(0u32);
    let bump = taps.clone();
    let height = taps.clone();
    let label = taps.clone();
    group()
        .key("counter")
        .width(200.0)
        .height(height.map(|n| 100.0 + n as f32 * 20.0))
        .fill(Color::hex(0xCA6D60))
        .radius(12.0)
        .on_click(move |_| bump.set(bump.get() + 1))
        .children([
            text(label.map(|n| format!("taps={n}")))
                .size(16.0)
                .color(Color::WHITE),
        ])
}

/// Hero panel: the box whose color to edit. Contains the counter.
#[component(name = "hot_ui::Hero")]
fn Hero() -> impl IntoElement {
    group()
        .key("hero")
        .direction(Direction::Horizontal)
        .width(460.0)
        .height(204.0)
        // EDIT ME: change this fill, save, and watch the running window.
        .fill(Color::hex(0xDC2626))
        .gap(16.0)
        .children([Counter()])
}

/// Page root: a white canvas holding the hero.
#[component(name = "hot_ui::Page")]
fn Page() -> impl IntoElement {
    group()
        .key("page")
        .direction(Direction::Vertical)
        .width(500.0)
        .height(560.0)
        .fill(Color::WHITE)
        .children([Hero()])
}

/// Protocol version the host checks before installing entry points.
#[no_mangle]
pub extern "Rust" fn aurora_hot_version() -> u32 {
    COMPONENT_ABI_VERSION
}

/// Rebuilds the interface. Re-run on every reload under the old hook scope.
#[no_mangle]
pub extern "Rust" fn aurora_component_root() -> aurora::dsl::Element {
    Page().into_element()
}

/// State keys and the values fresh keys start at. Live values always win over these.
/// Remote-protocol version the host checks before installing entry points.
#[no_mangle]
pub extern "Rust" fn aurora_remote_version() -> u32 {
    REMOTE_ABI_VERSION
}

#[no_mangle]
pub extern "Rust" fn aurora_remote_state() -> RemoteStateDecl {    RemoteStateDecl {
        f32s: vec![("taps".to_string(), 0.0)],
        strings: vec![],
        bools: vec![("open".to_string(), false)],
    }
}

/// The interface. Branch on `snap` freely — structure is data, so any edit applies live.
#[no_mangle]
pub extern "Rust" fn aurora_remote_build(snap: &RemoteSnapshot) -> RemoteNode {
    let mut root = RemoteNode::group("root");
    root.direction = RemoteDirection::Vertical;
    root.width = RemoteSize::Fill;
    root.height = RemoteSize::Fill;
    root.fill = Some(0xFFFFFF);

    // EDIT ME: this is the box whose color to change. Save and watch the window.
    let mut hero = RemoteNode::group("hero");
    hero.width = RemoteSize::Fixed(460.0);
    hero.height = RemoteSize::Fixed(204.0);
    hero.fill = Some(0xDC2626);
    hero.direction = RemoteDirection::Horizontal;
    hero.gap = 16.0;

    let mut swatch = RemoteNode::group("swatch");
    swatch.width = RemoteSize::Fixed(127.0);
    swatch.height = RemoteSize::Fixed(96.0);
    swatch.fill = Some(0x4900FF);
    let mut hello = RemoteNode::text("hello", "");
    hello.text = Some(RemoteText::Literal("Hello".to_string()));
    hello.text_size = 16.0;
    hello.text_color = Some(0xFFFFFF);
    swatch.children.push(hello);
    hero.children.push(swatch);

    // Counter card: click it and "taps" survives every reload below.
    let mut card = RemoteNode::group("card");
    card.width = RemoteSize::Fixed(200.0);
    card.height = RemoteSize::Fixed(120.0);
    card.fill = Some(0xCA6D60);
    card.radius = 12.0;
    card.on_click = Some("tap".to_string());
    let mut taps = RemoteNode::text("taps", "");
    taps.text = Some(RemoteText::BoundF32 {
        path: "taps".to_string(),
        prefix: "taps=".to_string(),
        suffix: String::new(),
    });
    taps.text_size = 16.0;
    taps.text_color = Some(0xFFFFFF);
    card.children.push(taps);
    hero.children.push(card);
    root.children.push(hero);

    // Toggle row: proves boolean state and conditional structure survive reloads.
    let mut row = RemoteNode::group("row");
    row.direction = RemoteDirection::Horizontal;
    row.width = RemoteSize::Fixed(460.0);
    row.height = RemoteSize::Fixed(60.0);
    row.gap = 12.0;
    let mut toggle = RemoteNode::group("toggle");
    toggle.width = RemoteSize::Fixed(200.0);
    toggle.height = RemoteSize::Fixed(60.0);
    toggle.fill = Some(if snap.get_bool("open") { 0x22C55E } else { 0x6B7280 });
    toggle.radius = 8.0;
    toggle.on_click = Some("toggle".to_string());
    let mut state = RemoteNode::text("state", "");
    state.text = Some(RemoteText::BoundBool {
        path: "open".to_string(),
        when_true: "Open".to_string(),
        when_false: "Closed".to_string(),
    });
    state.text_size = 14.0;
    state.text_color = Some(0xFFFFFF);
    toggle.children.push(state);
    row.children.push(toggle);
    root.children.push(row);

    root
}

/// Named click actions become state writes the host applies to its signals.
#[no_mangle]
pub extern "Rust" fn aurora_remote_event(action: &str) -> Vec<StateWrite> {
    match action {
        "tap" => vec![StateWrite::AddF32("taps".to_string(), 1.0)],
        "toggle" => vec![StateWrite::ToggleBool("open".to_string())],
        _ => Vec::new(),
    }
}
