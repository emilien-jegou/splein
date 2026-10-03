// Single responsibility: Shared pointer tracking driving per-demo update closures.
#![allow(dead_code)]

use aurora::prelude::*;
use winit::event::{ElementState, WindowEvent};

/// Pointer state captured from window events and resolved against the view tree.
pub struct Pointer {
    /// Last known cursor position in logical window pixels.
    pub cursor: Option<(f32, f32)>,
    /// Whether the primary button went down since the last update.
    pub clicked: bool,
    /// Whether the primary button came up since the last update.
    pub released: bool,
    /// Keyed view currently under the cursor, if any.
    pub over: Option<String>,
}

impl Pointer {
    /// Creates a pointer with no position and no pending edges.
    pub fn new() -> Self {
        Self {
            cursor: None,
            clicked: false,
            released: false,
            over: None,
        }
    }

    /// Whether the cursor rests on a particular keyed view.
    pub fn over_key(&self, key: &str) -> bool {
        self.over.as_deref() == Some(key)
    }
}

/// Extension running a closure before every frame with live pointer state.
pub struct Interactive<F> {
    /// Pointer state refreshed from window events and hit-testing.
    pub pointer: Pointer,
    update: F,
}

impl<F> Interactive<F> {
    /// Wraps a closure receiving the engine and pointer state each frame.
    pub fn new(update: F) -> Self {
        Self {
            pointer: Pointer::new(),
            update,
        }
    }
}

impl<F: FnMut(&mut Engine, &mut Pointer) -> bool> AppExtension for Interactive<F> {
    fn on_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer.cursor = Some((position.x as f32, position.y as f32));
                true
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer.cursor = None;
                true
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                self.pointer.clicked = true;
                true
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                ..
            } => {
                self.pointer.released = true;
                true
            }
            _ => false,
        }
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        self.pointer.over = self
            .pointer
            .cursor
            .and_then(|(x, y)| engine.hit_test(x, y))
            .map(|key| key.0);

        let wants_frames = (self.update)(engine, &mut self.pointer);
        self.pointer.clicked = false;
        self.pointer.released = false;
        wants_frames || engine.has_active_motion()
    }
}
