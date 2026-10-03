// Single responsibility: Entrypoint and hover/click driver for the playback demo.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/interactive.rs"]
mod interactive;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

mod controls;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::motion::MotionVector;
use aurora::prelude::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use common::ExampleCli;
use motion_ui::*;
use winit::event::{ElementState, WindowEvent};

pub use controls::build_ui;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 440;
const DURATION: f32 = 2.4;
const TRAVEL: f32 = 180.0;

/// Slides a keyed card in a loop, holding still while the pointer rests on it.
struct SlideDriver {
    outward: bool,
    cursor: Option<(f32, f32)>,
    clicked: bool,
    hover_paused: bool,
    status: Signal<String>,
}

impl AppExtension for SlideDriver {
    fn on_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x as f32, position.y as f32));
                true
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                true
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                self.clicked = true;
                true
            }
            _ => false,
        }
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        let under = |engine: &mut Engine, cursor: Option<(f32, f32)>| {
            cursor.and_then(|(x, y)| engine.hit_test(x, y)).map(|key| key.0)
        };

        if self.clicked {
            self.clicked = false;
            if under(engine, self.cursor).as_deref() == Some("restart") {
                if let Some(controller) = engine.controller("card") {
                    controller.cancel();
                }
                let at = engine
                    .motion_state("card")
                    .map(|state| MotionVector::from_state(&state).x)
                    .unwrap_or(0.0);
                self.outward = at < TRAVEL * 0.5;
                self.hover_paused = false;
                self.start(engine);
                self.status.set(String::from("restarted"));
            }
        }

        let over = under(engine, self.cursor).as_deref() == Some("card");
        if over && !self.hover_paused {
            if let Some(controller) = engine.controller("card") {
                controller.pause();
                self.hover_paused = true;
                self.status.set(String::from("paused · hover"));
            }
        } else if !over && self.hover_paused {
            self.hover_paused = false;
            if let Some(controller) = engine.controller("card") {
                controller.resume();
                self.status.set(String::from("playing"));
            }
        }

        // Frames are only requested while the card is actually moving.
        if !engine.has_active_motion() && !over {
            self.outward = !self.outward;
            self.start(engine);
            self.status.set(String::from("playing"));
        }

        engine.has_active_motion()
    }
}

impl SlideDriver {
    /// Starts the next slide toward the far end of the arena.
    fn start(&self, engine: &mut Engine) {
        let x = if self.outward { TRAVEL } else { 0.0 };
        engine
            .animate("card")
            .x(x)
            .duration(DURATION)
            .ease(Ease::InOutCubic)
            .play();
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let status = Signal::new(Rc::clone(&runtime), String::from("playing"));

    cli.app_config("Aurora — Motion 03d · Hover to pause, click to restart")
        .font("Inter", include_bytes!("../../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(SlideDriver {
            outward: true,
            cursor: None,
            clicked: false,
            hover_paused: false,
            status: status.clone(),
        })
        .run(move || build_ui(status.clone()));
}
