// Single responsibility: Interactive showcase for native text selection.
//
// Exercises every implemented selection feature in one window: press-drag
// inside a run, drags spanning several runs, double-click words,
// triple-click paragraphs, Shift-click extension, and Ctrl+C copy-out to the
// OS clipboard. The status card below the copy is a live readout driven by an
// `AppExtension` polling the engine, so what you see is what the engine holds.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::app::switcher::{BootCtx, BootedDemo};
use aurora::app::{AppExtension, VisualDebugger};
use aurora::dsl::component::ProviderProps;
use aurora::initContext;
use aurora::prelude::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;

/// Live selection readout, provided by [`Page`] and consumed by [`status_card`].
/// A separate object from [`CLIPBOARD`] over the same value type — identity is
/// the object, not the type.
static STATUS: Context<String> = initContext!();
/// OS clipboard readout; see [`STATUS`].
static CLIPBOARD: Context<String> = initContext!();

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/motion_ui.rs"]
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
            format!(
                "Selection ({} run{}): {}",
                runs.len(),
                if runs.len() == 1 { "" } else { "s" },
                parts.join("  ⏎  ")
            )
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

/// Rebuilds the selection world on a fresh runtime for in-window switching.
///
/// The standalone main adds its own debugger; here the switcher already owns
/// the shared one, so only the selection monitor is installed.
pub(crate) fn boot(ctx: &BootCtx) -> BootedDemo {
    let runtime = Rc::clone(&ctx.runtime);
    let status = Signal::new(Rc::clone(&runtime), String::new());
    let clipboard = Signal::new(Rc::clone(&runtime), String::new());
    let monitor = SelectionMonitor::new(status, clipboard);

    let cx = Cx::new(runtime);
    let root = cx
        .enter_keyed("root", move || Page(status, clipboard))
        .into_element();
    BootedDemo {
        root,
        background: 0xF4F5F7u32.into(),
        extensions: vec![Box::new(monitor)],
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let status = Signal::new(Rc::clone(&runtime), String::new());
    let clipboard = Signal::new(Rc::clone(&runtime), String::new());
    let monitor = SelectionMonitor::new(status, clipboard);
    let mut debugger = VisualDebugger::new();
    debugger.show_hud = cli.show_hud;

    cli.app_config_multi("Aurora — Text · Selection showcase")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .runtime(runtime)
        .extension(debugger)
        .extension(monitor)
        .run(move || Page(status, clipboard));
}

/// Page body: selectable copy, a live status card, and the gesture cheat sheet.
///
/// Provides the monitor's readout signals as context rather than drilling them:
/// `status_card` reads them where it renders, with no intermediate forwarding.
#[component]
pub fn Page(status: Signal<String>, clipboard: Signal<String>) -> impl IntoElement {
    view! {
        <Provider grant={STATUS.provide(status)}>
            <Provider grant={CLIPBOARD.provide(clipboard)}>
                <Shell
                    eyebrow_label="TEXT · 01 — SELECTION"
                    title="One gesture, many runs."
                    copy="Every paragraph below is selectable body copy. Drag inside one, or sweep across several: the highlight spans runs, Shift-click extends from the anchor, and Ctrl+C copies to the OS clipboard."
                >
                    <Content>
                        <Group direction="vertical" width="fill" gap={12.0}>
                            <Para
                                hk="show-a"
                                copy="Aurora keeps one layout and compositor engine under a reactive DSL, so a paragraph is shaped once and every caret, word edge, and highlight rect derives from that same layout."
                            />
                            <Para
                                hk="show-b"
                                copy="Transforms and opacity are compositor overrides: they invalidate paint only and never reach the layout pass, which is why dragging a selection over animated chrome stays exact."
                            />
                            <Para
                                hk="show-c"
                                copy="A spring drives a signal, layout reads that signal, and the engine only lays out and paints what it is given — press into the copy while panels animate and the anchor still resolves."
                            />
                            <StatusCard />
                            <CheatSheet />
                        </Group>
                    </Content>
                </Shell>
            </Provider>
        </Provider>
    }
}

#[component]
fn Para(hk: &'static str, copy: &'static str) -> impl IntoElement {
    view! {
        <Text
            key={hk}
            selectable={true}
            size={13.0}
            line_height={20.0}
            color={BODY}
            width="fill"
        >{copy}</Text>
    }
}

#[component]
fn StatusCard() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            gap={6.0}
            margin={16.0}
            radius={16.0}
            fill={SURFACE}
            stroke={Stroke::inside(1.0, hairline())}
        >
            <Eyebrow label="LIVE SELECTION" />
            <Text size={12.0} color={INK}>{STATUS.get()}</Text>
            <Eyebrow label="OS CLIPBOARD" />
            <Text size={12.0} color={INK}>{CLIPBOARD.get()}</Text>
        </Group>
    }
}

#[component]
fn CheatSheet() -> impl IntoElement {
    view! {
        <Group direction="vertical" gap={2.0}>
            <Hint copy="drag — select chars · across runs spans them" />
            <Hint copy="double-click — word · triple-click — paragraph" />
            <Hint copy="shift+click — extend from the anchor" />
            <Hint copy="ctrl+C — copy to OS clipboard · escape — collapse" />
        </Group>
    }
}

#[component]
fn Hint(copy: &'static str) -> impl IntoElement {
    view! {
        <Text size={11.0} color={MUTED}>{copy}</Text>
    }
}
