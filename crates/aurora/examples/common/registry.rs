// Single responsibility: Registry of every switchable demo plus the F5 menu page.
//
// Included once per example binary at the crate root (`#[path =
// "common/registry.rs"] mod registry;`). Each demo file is included as a
// submodule, so no demo source changes except visibility tweaks and the four
// stateful demos' `boot` functions (moving, showcase, selection_showcase,
// shadow_studio), which live next to their mains.

#[allow(dead_code)]
#[path = "../demo1.rs"]
mod demo1;
#[allow(dead_code)]
#[path = "../demo2.rs"]
mod demo2;
#[allow(dead_code)]
#[path = "../motion_imperative.rs"]
mod motion_imperative;
#[allow(dead_code)]
#[path = "../motion_interruption.rs"]
mod motion_interruption;
#[allow(dead_code)]
#[path = "../motion_layout_split.rs"]
mod motion_layout_split;
#[allow(dead_code)]
#[path = "../motion_press.rs"]
mod motion_press;
#[allow(dead_code)]
#[path = "../motion_reorder.rs"]
mod motion_reorder;
#[allow(dead_code)]
#[path = "../motion_signals.rs"]
mod motion_signals;
#[allow(dead_code)]
#[path = "../motion_transitions.rs"]
mod motion_transitions;
#[allow(dead_code)]
#[path = "../motion_validate.rs"]
mod motion_validate;
#[allow(dead_code)]
#[path = "../moving.rs"]
mod moving;
#[allow(dead_code)]
#[path = "../selectable_text.rs"]
mod selectable_text;
#[allow(dead_code)]
#[path = "../selection_showcase.rs"]
mod selection_showcase;
#[allow(dead_code)]
#[path = "../shadow_lab.rs"]
mod shadow_lab;
#[allow(dead_code)]
#[path = "../shadow_studio.rs"]
mod shadow_studio;
#[allow(dead_code)]
#[path = "../showcase.rs"]
mod showcase;
#[allow(dead_code)]
#[path = "../text_demo.rs"]
mod text_demo;
#[allow(dead_code)]
#[path = "../text_smooth.rs"]
mod text_smooth;
#[allow(dead_code)]
#[path = "../view_demo.rs"]
mod view_demo;

#[path = "motion_ui.rs"]
mod motion_ui;

use std::rc::Rc;

use aurora::app::multibackend::MultiBackend;
use aurora::app::switcher::{
    BootCtx, BootedDemo, DemoDef, DemoMeta, MenuAction, MenuState,
};
use aurora::app::VisualDebugger;
use aurora::prelude::*;
use motion_ui::*;

use crate::common::ExampleCli;

/// Builds a root element under a stable `"root"` hook scope on the given runtime.
fn mount_root<F, E>(ctx: &BootCtx, make: F) -> Element
where
    F: FnOnce() -> E,
    E: IntoElement,
{
    let cx = Cx::new(Rc::clone(&ctx.runtime));
    cx.enter_keyed("root", make).into_element()
}

/// Every switchable demo (menu excluded; the runner adds it separately).
pub fn all() -> Vec<DemoDef> {
    vec![
        DemoDef { id: "demo1", title: "Demo 1 · Font manager", boot: boot_demo1 },
        DemoDef { id: "demo2", title: "Demo 2 · HUD dock", boot: boot_demo2 },
        DemoDef { id: "showcase", title: "Mission Control showcase", boot: showcase::boot },
        DemoDef { id: "moving", title: "DVD bounce", boot: moving::boot },
        DemoDef { id: "motion-press", title: "Press feedback", boot: boot_motion_press },
        DemoDef { id: "motion-transitions", title: "Signal transitions", boot: boot_motion_transitions },
        DemoDef { id: "motion-validate", title: "Growing action", boot: boot_motion_validate },
        DemoDef { id: "motion-reorder", title: "Drag reorder", boot: boot_motion_reorder },
        DemoDef { id: "motion-layout-split", title: "Layout vs compositor", boot: boot_motion_layout_split },
        DemoDef { id: "motion-imperative", title: "Reactive targets", boot: boot_motion_imperative },
        DemoDef { id: "motion-interruption", title: "Seamless interruption", boot: boot_motion_interruption },
        DemoDef { id: "motion-signals", title: "Signal-driven motion", boot: boot_motion_signals },
        DemoDef { id: "text-smooth", title: "Smooth drift repro", boot: boot_text_smooth },
        DemoDef { id: "text-demo", title: "Text layout", boot: boot_text_demo },
        DemoDef { id: "selectable-text", title: "Selectable text", boot: boot_selectable_text },
        DemoDef { id: "selection-showcase", title: "Selection showcase", boot: selection_showcase::boot },
        DemoDef { id: "shadow-lab", title: "Shadow lab", boot: boot_shadow_lab },
        DemoDef { id: "shadow-studio", title: "Shadow studio", boot: shadow_studio::boot },
        DemoDef { id: "view-demo", title: "view! demo", boot: boot_view_demo },
    ]
}

/// The F5 menu page boot (id `__menu`, never listed).
pub fn menu_def() -> DemoDef {
    DemoDef { id: aurora::app::switcher::MENU_ID, title: "Demo menu", boot: boot_menu }
}

fn boot_demo1(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, demo1::build_ui),
        background: Color::WHITE,
        extensions: vec![],
    }
}

fn boot_demo2(ctx: &BootCtx) -> BootedDemo {
    let icons = demo2::Icons::load();
    BootedDemo {
        root: mount_root(ctx, || demo2::build_ui(&icons)),
        background: 0x141414.into(),
        extensions: vec![],
    }
}

fn boot_motion_press(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_press::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_transitions(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_transitions::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_validate(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_validate::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_reorder(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_reorder::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_layout_split(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_layout_split::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_imperative(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_imperative::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_interruption(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_interruption::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_motion_signals(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, motion_signals::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_text_smooth(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, text_smooth::Page),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_text_demo(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, text_demo::build_ui),
        background: 0x0B1020.into(),
        extensions: vec![],
    }
}

fn boot_selectable_text(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, selectable_text::build_ui),
        background: Color::WHITE,
        extensions: vec![],
    }
}

fn boot_shadow_lab(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, shadow_lab::build_ui),
        background: shadow_lab::PANEL_BG.into(),
        extensions: vec![],
    }
}

fn boot_view_demo(ctx: &BootCtx) -> BootedDemo {
    BootedDemo {
        root: mount_root(ctx, view_demo::PageEntry),
        background: PAGE.into(),
        extensions: vec![],
    }
}

fn boot_menu(ctx: &BootCtx) -> BootedDemo {
    let state = ctx.menu.clone().expect("menu boot needs menu state");
    BootedDemo {
        root: mount_root(ctx, || MenuPage(ctx.demos.clone(), state.clone())),
        background: Color::BLACK,
        extensions: vec![],
    }
}

/// Entry point shared by every example main: `registry::run_demo(cli, "motion-press")`.
pub fn run_demo(cli: ExampleCli, id: &'static str) {
    let mut debugger = VisualDebugger::new();
    debugger.show_hud = cli.show_hud;
    MultiBackend::config([
        Some(aurora_tiny_skia::create_backend),
        Some(aurora_vello::create_backend),
        Some(aurora_vello::create_cpu_backend),
        Some(aurora_wgpu::create_backend),
    ])
    .quad_factory(aurora_quad::create_backend)
    .title("Aurora")
    .size(cli.width, cli.height)
    .initial(cli.backend)
    .show_hud(cli.show_hud)
    .fps_limit(cli.fps_limit)
    .extension(debugger)
    .run_switchable(all(), menu_def(), id);
}

/// Grub-style menu: black screen, centered white text, keyboard navigable.
/// Single vertical list; the highlighted row inverts (white block, black
/// text). Mouse works too.
#[component]
pub fn MenuPage(demos: Vec<DemoMeta>, state: MenuState) -> impl IntoElement {
    let rows: Vec<Element> = demos
        .iter()
        .enumerate()
        .map(|(idx, demo)| menu_row(demo.title, idx, &state))
        .collect();
    let pend = state.pending.clone();
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Color::BLACK}
            alignment="center"
            distribution="center"
            gap={8.0}
        >
            <Text size={20.0} weight={700u16} color={Color::WHITE}>
                "AURORA — SELECT DEMO"
            </Text>
            <Text size={12.0} color={0x888888}>
                "up/down navigate · enter select · esc back"
            </Text>
            <Group direction="vertical" gap={4.0} alignment="center">
                {rows}
            </Group>
            <Group
                width={200.0}
                height={30.0}
                radius={6.0}
                alignment="center"
                distribution="center"
                on:click={{
                    move |_: &ClickInfo| pend.set(Some(MenuAction::Back))
                }}
            >
                <Text size={13.0} color={0x888888}>{"< back (esc)"}</Text>
            </Group>
        </Group>
    }
}

/// One grub row: white text, inverted block when highlighted.
///
/// Text ink is not a reactive prop, so the row carries twin labels and
/// flips visibility (the same pattern as conditional panels elsewhere).
fn menu_row(title: &'static str, idx: usize, state: &MenuState) -> Element {
    let hov = state.selected.clone();
    let pend = state.pending.clone();
    let bg = state.selected.clone().map(move |s| {
        if s == idx { Color::WHITE } else { Color::BLACK }
    });
    let is_sel = state.selected.clone().map(move |s| s == idx);
    let is_dim = state.selected.clone().map(move |s| s != idx);
    let launch = move |_: &ClickInfo| {
        pend.set(Some(MenuAction::Launch(idx)));
    };
    view! {
        <Group
            direction="horizontal"
            width={440.0}
            height={30.0}
            radius={6.0}
            fill={bg}
            alignment="center"
            distribution="center"
            on:click={launch}
            on:pointer-enter={{
                move |_: &HoverInfo| hov.set(idx)
            }}
        >
            <Group direction="horizontal" visible={is_dim} alignment="center" distribution="center">
                <Text size={14.0} weight={700u16} color={Color::WHITE}>{title}</Text>
            </Group>
            <Group direction="horizontal" visible={is_sel} alignment="center" distribution="center">
                <Text size={14.0} weight={700u16} color={Color::BLACK}>{title}</Text>
            </Group>
        </Group>
    }
}
