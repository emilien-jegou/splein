// Single responsibility: Master demo launcher — one fullscreen menu that runs every demo.
//
// Each demo stays its own process (exact setup, own windows, multi-backend
// keys included): selecting a row spawns the sibling example binary
// fire-and-forget with the gallery's backend arg forwarded.
// Keyboard: Up/Down/Home/End move, Enter/Space launches, Esc exits.
// Mouse: hover highlights, click launches. Esc inside any child demo
// exits it (bound in both runners) and drops you back here.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

use std::cell::RefCell;
use std::rc::Rc;

use riode::app::{AppExtension, VisualDebugger};
use riode::prelude::*;
use riode::reactive::ReactiveRuntime;
use common::ExampleCli;
use motion_ui::*;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{Key, NamedKey};

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 800;

struct DemoEntry {
    binary: &'static str,
    title: &'static str,
    blurb: &'static str,
}

const DEMOS: &[DemoEntry] = &[
    DemoEntry {
        binary: "demo1",
        title: "Demo 1 · Optype Font Manager",
        blurb: "Font management HUD panel.",
    },
    DemoEntry {
        binary: "demo2",
        title: "Demo 2 · Paper HUD Dock",
        blurb: "Self-contained dock runner.",
    },
    DemoEntry {
        binary: "showcase",
        title: "Mission Control Showcase",
        blurb: "Full engine showcase with live ticker.",
    },
    DemoEntry {
        binary: "moving",
        title: "DVD Bounce Screensaver",
        blurb: "Physics driver extension demo.",
    },
    DemoEntry {
        binary: "motion_press",
        title: "Web 01 · Press feedback",
        blurb: "90ms squeeze, spring release.",
    },
    DemoEntry {
        binary: "motion_transitions",
        title: "Motion 02 · Signal transitions",
        blurb: "Transitions over signals.",
    },
    DemoEntry {
        binary: "motion_validate",
        title: "Web 03 · Growing action",
        blurb: "Action that grows.",
    },
    DemoEntry {
        binary: "motion_reorder",
        title: "Web 04 · Drag to reorder",
        blurb: "FLIP reorder list.",
    },
    DemoEntry {
        binary: "motion_layout_split",
        title: "Motion 01 · Layout vs compositor",
        blurb: "Layout motion vs transform.",
    },
    DemoEntry {
        binary: "motion_imperative",
        title: "Motion 03 · Reactive targets",
        blurb: "Imperative motion targets.",
    },
    DemoEntry {
        binary: "motion_interruption",
        title: "Motion 03c · Interruption",
        blurb: "Seamless retargeting mid-flight.",
    },
    DemoEntry {
        binary: "motion_signals",
        title: "Motion 05 · Signal-driven",
        blurb: "Signals driving motion.",
    },
    DemoEntry {
        binary: "scroll_demo",
        title: "Scroll · Drift repro",
        blurb: "Clip viewport + absolute content triage.",
    },
    DemoEntry {
        binary: "text_smooth",
        title: "Text · Smooth drift repro",
        blurb: "Subpixel animation triage.",
    },
    DemoEntry {
        binary: "text_demo",
        title: "Text · Layout",
        blurb: "Text shaping and layout.",
    },
    DemoEntry {
        binary: "selectable_text",
        title: "Text · Selectable",
        blurb: "Pointer selection basics.",
    },
    DemoEntry {
        binary: "selection_showcase",
        title: "Text · Selection showcase",
        blurb: "Drags, words, Shift-click, Ctrl+C.",
    },
    DemoEntry {
        binary: "shadow_lab",
        title: "Shadow Lab",
        blurb: "Shadow parameter playground.",
    },
    DemoEntry {
        binary: "shadow_studio",
        title: "Shadow Studio",
        blurb: "Full shadow authoring.",
    },
    DemoEntry {
        binary: "view_demo",
        title: "view! demo",
        blurb: "Typed-props components + identity.",
    },
];

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let backend_name = match cli.backend {
        RenderBackend::TinySkia => "tiny-skia",
        RenderBackend::Skia => "skia",
        RenderBackend::Vello => "vello-gpu",
        RenderBackend::VelloCpu => "vello-cpu",
        RenderBackend::Wgpu => "wgpu",
    };
    let child_args = vec![backend_name.to_string()];

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let selected = Signal::new(Rc::clone(&runtime), 0usize);
    let status = Signal::new(
        Rc::clone(&runtime),
        "↑↓ navigate · Enter/click launches · Esc exits".to_string(),
    );
    let nav = GalleryNav::new(selected, status, child_args.clone());

    cli.app_config_multi("Aurora — Demo Gallery")
        .fullscreen(true)
        .font(
            "Inter",
            include_bytes!("../../crates/core/riode/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .runtime(runtime)
        .extension(nav)
        .run(move || GalleryPage(selected, status, child_args.clone()));
}

/// Spawns a sibling example binary, detached. Gallery stays open as the panel.
fn spawn_demo(binary: &str, args: &[String]) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe.parent().ok_or("no parent dir")?;
    #[cfg(windows)]
    let file = format!("{binary}.exe");
    #[cfg(not(windows))]
    let file = binary.to_string();
    let path = dir.join(&file);
    if !path.exists() {
        return Err(format!("{file} not built — cargo build --example {binary}"));
    }
    match std::process::Command::new(&path).args(args).spawn() {
        Ok(child) => {
            std::mem::forget(child);
            Ok(format!("launched {binary}"))
        }
        Err(e) => Err(format!("spawn {binary}: {e}")),
    }
}

/// Keyboard navigation for the gallery menu (the component owns the mouse).
pub struct GalleryNav {
    selected: Signal<usize>,
    status: Signal<String>,
    args: Vec<String>,
}

impl GalleryNav {
    fn new(selected: Signal<usize>, status: Signal<String>, args: Vec<String>) -> Self {
        Self {
            selected,
            status,
            args,
        }
    }

    fn launch_selected(&self) -> bool {
        let idx = self.selected.get();
        if let Some(demo) = DEMOS.get(idx) {
            let msg = match spawn_demo(demo.binary, &self.args) {
                Ok(ok) => ok,
                Err(err) => err,
            };
            self.status.set(msg);
        }
        true
    }
}

impl AppExtension for GalleryNav {
    fn on_event(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput { event: key, .. } = event else {
            return false;
        };
        if key.state != ElementState::Pressed {
            return false;
        }
        let n = DEMOS.len();
        match &key.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                self.selected.update(|s| *s = (*s + 1) % n);
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.selected.update(|s| *s = (*s + n - 1) % n);
                true
            }
            Key::Named(NamedKey::Home) => {
                self.selected.set(0);
                true
            }
            Key::Named(NamedKey::End) => {
                self.selected.set(n - 1);
                true
            }
            Key::Named(NamedKey::Enter) => self.launch_selected(),
            Key::Character(c) if c.as_str() == " " => self.launch_selected(),
            _ => false,
        }
    }
}

/// Fullscreen menu: two columns of clickable rows + a status line.
#[component]
pub fn GalleryPage(
    selected: Signal<usize>,
    status: Signal<String>,
    args: Vec<String>,
) -> impl IntoElement {
    let half = DEMOS.len().div_ceil(2);
    let left = demo_column(&selected, &status, &args, 0, half);
    let right = demo_column(&selected, &status, &args, half, DEMOS.len());
    view! {
        <Shell
            eyebrow_label="GALLERY · ALL DEMOS"
            title="One door, every demo."
            copy="↑↓ moves · Enter launches · hover + click works too · Esc exits a demo back here, Esc here quits. 1-5 / Tab / F7 inside each demo still switch engines."
        >
            <Content>
                <Group direction="vertical" width="fill" height="fill" gap={12.0}>
                    <Group direction="horizontal" width="fill" height="fill" gap={12.0}>
                        <Group direction="vertical" width="fill" height="fill" gap={8.0}>
                            {left}
                        </Group>
                        <Group direction="vertical" width="fill" height="fill" gap={8.0}>
                            {right}
                        </Group>
                    </Group>
                    <Group
                        width="fill"
                        height={30.0}
                        radius={10.0}
                        fill={CHIP}
                        alignment="center"
                    >
                        <Text size={12.0} color={Color::WHITE}>
                            {status.map(|s| s)}
                        </Text>
                    </Group>
                </Group>
            </Content>
        </Shell>
    }
}

fn demo_column(
    selected: &Signal<usize>,
    status: &Signal<String>,
    args: &[String],
    from: usize,
    to: usize,
) -> Vec<Element> {
    DEMOS[from..to]
        .iter()
        .enumerate()
        .map(|(k, demo)| {
            let idx = from + k;
            let hov = selected.clone();
            let st = status.clone();
            let owned_args: Vec<String> = args.to_vec();
            let binary = demo.binary;
            let bg = selected.clone().map(move |s| {
                if s == idx {
                    Color::hex(0xE4E9F5)
                } else {
                    Color::hex(SURFACE)
                }
            });
            let launch = move |_: &ClickInfo| {
                st.set(match spawn_demo(binary, &owned_args) {
                    Ok(ok) => ok,
                    Err(err) => err,
                });
            };
            view! {
                <Group
                    direction="horizontal"
                    width="fill"
                    height={52.0}
                    radius={12.0}
                    fill={bg}
                    stroke={Stroke::inside(1.0, hairline())}
                    alignment="center"
                    gap={12.0}
                    margin={Margin::x(12.0)}
                    on:click={launch}
                    on:pointer-enter={{
                        move |_| hov.set(idx)
                    }}
                >
                    <Text size={13.0} weight={700u16} color={INK}>{demo.title}</Text>
                    <Text size={12.0} color={BODY}>{demo.blurb}</Text>
                </Group>
            }
        })
        .collect()
}
