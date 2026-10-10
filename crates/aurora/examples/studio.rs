// Single responsibility: Switchable master binary hosting every demo in one window.
//
// F5 opens the grub-style demo menu, Enter/click switches live in the
// current window, F6 toggles quad combine, F7 cycles raster engines,
// Esc goes back / exits. Optional positional demo id selects the start
// demo: `cargo run --example studio -- motion-press`.

#[path = "common/mod.rs"]
mod common;
#[path = "common/registry.rs"]
mod registry;

use common::ExampleCli;

fn main() {
    let cli = ExampleCli::parse(1280, 800);
    cli.init_telemetry();
    // First positional arg that names a demo wins as the start demo;
    // anything else is the engine selector the CLI already parsed.
    let start = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .find(|a| registry::all().iter().any(|d| d.id == a.as_str()))
        .unwrap_or_else(|| "text-smooth".to_string());
    // `all()` allocates per call; leak one copy for the 'static id.
    let initial: &'static str = Box::leak(start.into_boxed_str());
    registry::run_demo(cli, initial);
}

#[cfg(test)]
mod switch_tests {
    use aurora::app::switcher::{BootCtx, DemoDef, DemoMeta, MenuState};
    use aurora::prelude::*;
    use aurora::reactive::ReactiveRuntime;
    use aurora::runtime::Engine;
    use aurora::scene::SceneCommand;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::registry;

    fn draw_text_count(engine: &Engine) -> usize {
        engine
            .scene()
            .chunks
            .iter()
            .flat_map(|c| c.commands.iter())
            .filter(|cmd| matches!(cmd, SceneCommand::DrawText { .. }))
            .count()
    }

    fn glyph_count(engine: &Engine) -> usize {
        engine
            .scene()
            .chunks
            .iter()
            .flat_map(|c| c.commands.iter())
            .filter_map(|cmd| match cmd {
                SceneCommand::DrawText { layout, .. } => Some(
                    layout.lines.iter().map(|l| l.glyphs.len()).sum::<usize>(),
                ),
                _ => None,
            })
            .sum()
    }

    fn meta_of(demos: &[DemoDef]) -> Vec<DemoMeta> {
        demos
            .iter()
            .enumerate()
            .map(|(index, d)| DemoMeta { index, title: d.title })
            .collect()
    }

    struct Session {
        runtime: Rc<RefCell<ReactiveRuntime>>,
        engine: Engine,
        demos: Vec<DemoDef>,
        meta: Vec<DemoMeta>,
        menu_state: MenuState,
    }

    impl Session {
        fn new(demo_id: &str) -> Self {
            let demos = registry::all();
            let meta = meta_of(&demos);
            let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
            let menu_state = MenuState {
                selected: Signal::new(Rc::clone(&runtime), 0usize),
                pending: Signal::new(Rc::clone(&runtime), None),
            };
            let demo = demos.iter().find(|d| d.id == demo_id).unwrap();
            let ctx = BootCtx {
                runtime: Rc::clone(&runtime),
                engine_w: 1280,
                engine_h: 800,
                demos: meta.clone(),
                menu: None,
            };
            let booted = (demo.boot)(&ctx);
            let mut engine = Engine::with_runtime(Rc::clone(&runtime), booted.root, 1280, 800);
            engine.set_background(booted.background);
            engine.load_font(include_bytes!("../assets/inter-var.ttf"));
            engine.frame();
            engine.frame();
            Self { runtime, engine, demos, meta, menu_state }
        }

        fn ctx(&self, menu: Option<MenuState>) -> BootCtx {
            BootCtx {
                runtime: Rc::clone(&self.runtime),
                engine_w: 1280,
                engine_h: 800,
                demos: self.meta.clone(),
                menu,
            }
        }

        fn switch(&mut self, id: &str) {
            self.engine.prepare_switch();
            let demo = self.demos.iter().find(|d| d.id == id).unwrap();
            let menu = if id == aurora::app::switcher::MENU_ID {
                Some(self.menu_state.clone())
            } else {
                None
            };
            let ctx = self.ctx(menu);
            let booted = (demo.boot)(&ctx);
            self.engine.switch_root(booted.root);
            self.engine.set_background(booted.background);
            self.engine.frame();
            self.engine.frame();
        }

        fn switch_menu(&mut self) {
            self.engine.prepare_switch();
            let ctx = self.ctx(Some(self.menu_state.clone()));
            let booted = (registry::menu_def().boot)(&ctx);
            self.engine.switch_root(booted.root);
            self.engine.set_background(booted.background);
            self.engine.frame();
            self.engine.frame();
        }
    }

    /// F5 round trip on ONE engine (the live switcher path): demo -> menu ->
    /// demo keeps shaping text, and the animated demo's tickers retire when
    /// it unmounts (no background engine) and resume on revisit (no frozen
    /// clock).
    #[test]
    fn f5_round_trip_single_engine() {
        let mut s = Session::new("text-smooth");
        assert!(draw_text_count(&s.engine) > 0, "demo shaped no text");
        assert!(glyph_count(&s.engine) > 0, "demo shaped no glyphs");
        assert!(
            s.engine.has_active_motion(),
            "animated demo should want frames"
        );

        s.switch_menu();
        assert!(draw_text_count(&s.engine) > 0, "menu shaped no text");
        assert!(glyph_count(&s.engine) > 0, "menu shaped no glyphs");
        assert!(
            !s.engine.has_active_motion(),
            "menu must not inherit the demo's tickers"
        );

        s.switch("text-smooth");
        assert!(draw_text_count(&s.engine) > 0, "returned demo shaped no text");
        assert!(glyph_count(&s.engine) > 0, "returned demo shaped no glyphs");
        assert!(
            s.engine.has_active_motion(),
            "returned demo's clock must tick again"
        );

        // Second visit still mounts fresh (no stale hook recall).
        s.switch_menu();
        s.switch("text-smooth");
        assert!(glyph_count(&s.engine) > 0, "second revisit shaped no glyphs");
        assert!(s.engine.has_active_motion(), "second revisit clock dead");
    }
}
