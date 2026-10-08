// Single responsibility: Interactive showcase for native text selection.
//
// Exercises every implemented selection feature in one window: press-drag
// inside a run, drags spanning several runs, double-click words,
// triple-click paragraphs, Shift-click extension, and Ctrl+C copy-out to the
// OS clipboard. The status card below the copy is a live readout driven by an
// `AppExtension` polling the engine, so what you see is what the engine holds.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::app::{AppExtension, VisualDebugger};
use aurora::prelude::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;

mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 760;

/// Shortens a readout to one line per run, so a whole paragraph selection
/// does not blow up the status card.
fn one_line(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() <= max {
        return flat;
    }
    let mut end = max;
    while end > 0 && !flat.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &flat[..end])
}

/// Polls selection + clipboard state into signals for the status card.
///
/// Sampling from `on_update` is the intended read pattern for non-reactive
/// engine state (same guidance as `ElementRef`): the write only fires on
/// change, so an idle window costs no frames.
pub struct SelectionMonitor {
    status: Signal<String>,
    clipboard: Signal<String>,
    last_status: String,
    last_clipboard: String,
}

impl SelectionMonitor {
    /// Creates a monitor driving the given readout signals.
    pub fn new(status: Signal<String>, clipboard: Signal<String>) -> Self {
        Self {
            status,
            clipboard,
            last_status: String::new(),
            last_clipboard: String::new(),
        }
    }
}

impl AppExtension for SelectionMonitor {
    fn on_update(&mut self, engine: &mut Engine) -> bool {
        let runs = engine.combined_selected_text();
        let status = if runs.is_empty() {
            "Selection: none — drag across the copy above.".to_string()
        } else {
            let parts: Vec<String> = runs.iter().map(|r| one_line(r, 48)).collect();
            format!("Selection ({} run{}): {}", runs.len(), if runs.len() == 1 { "" } else { "s" }, parts.join("  ⏎  "))
        };
        let clipboard = match engine.clipboard_text() {
            Some(text) => format!("Clipboard: {}", one_line(&text, 64)),
            None => "Clipboard: — (select, then Ctrl+C)".to_string(),
        };
        let mut redraw = false;
        if status != self.last_status {
            self.last_status = status.clone();
            self.status.set(status);
            redraw = true;
        }
        if clipboard != self.last_clipboard {
            self.last_clipboard = clipboard.clone();
            self.clipboard.set(clipboard);
            redraw = true;
        }
        redraw
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let status = Signal::new(Rc::clone(&runtime), String::new());
    let clipboard = Signal::new(Rc::clone(&runtime), String::new());
    let monitor = SelectionMonitor::new(status.clone(), clipboard.clone());
    let mut debugger = VisualDebugger::new();
    debugger.show_hud = cli.show_hud;

    cli.app_config("Aurora — Text · Selection showcase")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(debugger)
        .extension(monitor)
        .run(move || Page(status.clone(), clipboard.clone()));
}

/// Page body: selectable copy, a live status card, and the gesture cheat sheet.
#[allow(non_snake_case)]
pub fn Page(status: Signal<String>, clipboard: Signal<String>) -> impl IntoElement {
    page(
        "TEXT · 01 — SELECTION",
        "One gesture, many runs.",
        "Every paragraph below is selectable body copy. Drag inside one, or sweep across several: the highlight spans runs, Shift-click extends from the anchor, and Ctrl+C copies to the OS clipboard.",
        content(
            group()
                .direction(Direction::Vertical)
                .width(Size::fill())
                .gap(12.0)
                .children((
                    para("show-a", "Aurora keeps one layout and compositor engine under a reactive DSL, so a paragraph is shaped once and every caret, word edge, and highlight rect derives from that same layout."),
                    para("show-b", "Transforms and opacity are compositor overrides: they invalidate paint only and never reach the layout pass, which is why dragging a selection over animated chrome stays exact."),
                    para("show-c", "A spring drives a signal, layout reads that signal, and the engine only lays out and paints what it is given — press into the copy while panels animate and the anchor still resolves."),
                    status_card(status, clipboard),
                    cheat_sheet(),
                )),
        ),
    )
}

fn para(key: &'static str, copy: &'static str) -> impl IntoElement {
    text(copy)
        .key(key)
        .selectable(true)
        .size(13.0)
        .line_height(20.0)
        .color(Color::hex(BODY))
        .width(Size::fill())
        .into_element()
}

fn status_card(status: Signal<String>, clipboard: Signal<String>) -> GroupDef {
    surface()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .gap(6.0)
        .margin(Margin::all(16.0))
        .children((
            eyebrow("LIVE SELECTION"),
            text(status).size(12.0).color(Color::hex(INK)),
            eyebrow("OS CLIPBOARD"),
            text(clipboard).size(12.0).color(Color::hex(INK)),
        ))
}

fn cheat_sheet() -> GroupDef {
    group().direction(Direction::Vertical).gap(2.0).children((
        hint("drag — select chars · across runs spans them"),
        hint("double-click — word · triple-click — paragraph"),
        hint("shift+click — extend from the anchor"),
        hint("ctrl+C — copy to OS clipboard · escape — collapse"),
    ))
}

fn hint(copy: &'static str) -> impl IntoElement {
    text(copy).size(11.0).color(Color::hex(MUTED)).into_element()
}
