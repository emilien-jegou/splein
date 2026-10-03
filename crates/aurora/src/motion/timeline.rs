// Single responsibility: Choreography scheduling several keyed animations on one timeline.

use crate::foundation::Key;
use crate::motion::controller::Controller;
use crate::motion::spring::Spring;
use crate::motion::timing::resolve_token;
use crate::motion::tween::Tween;
use crate::motion::vector::MotionVector;
use crate::motion::Ease;
use crate::runtime::FrameScheduler;

/// One pending animation and the instant it should start.
struct Scheduled {
    start: f32,
    key: Key,
    to: MotionVector,
    tween: Tween,
    spring: Option<Spring>,
}

/// Fluent builder scheduling several keyed animations against one clock.
pub struct Timeline<'a> {
    scheduler: &'a mut FrameScheduler,
    entries: Vec<Scheduled>,
    labels: Vec<(&'static str, f32)>,
    cursor: f32,
}

impl<'a> Timeline<'a> {
    /// Creates an empty timeline starting at zero seconds.
    pub fn new(scheduler: &'a mut FrameScheduler) -> Self {
        Self {
            scheduler,
            entries: Vec::new(),
            labels: Vec::new(),
            cursor: 0.0,
        }
    }

    /// Records the current instant under a name later entries can jump back to.
    pub fn label(mut self, name: &'static str) -> Self {
        self.labels.push((name, self.cursor));
        self
    }

    /// Starts a new entry for a keyed view at the current instant.
    pub fn to(mut self, key: impl Into<Key>) -> Self {
        self.entries.push(Scheduled {
            start: self.cursor,
            key: key.into(),
            to: MotionVector::NEUTRAL,
            tween: Tween::new(0.3, Ease::OutCubic),
            spring: None,
        });
        self
    }

    /// Targets the horizontal translation of the current entry.
    pub fn x(mut self, value: f32) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.to.x = value;
        }
        self
    }

    /// Targets the vertical translation of the current entry.
    pub fn y(mut self, value: f32) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.to.y = value;
        }
        self
    }

    /// Targets the uniform scale of the current entry.
    pub fn scale(mut self, value: f32) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.to.scale_x = value;
            entry.to.scale_y = value;
        }
        self
    }

    /// Targets the opacity of the current entry.
    pub fn opacity(mut self, value: f32) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.to.opacity = value;
        }
        self
    }

    /// Sets the duration of the current entry in seconds.
    pub fn duration(mut self, seconds: f32) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.tween.duration = seconds;
        }
        self
    }

    /// Sets the easing curve of the current entry.
    pub fn ease(mut self, ease: Ease) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.tween.ease = ease;
        }
        self
    }

    /// Settles the current entry with physics instead of a fixed duration.
    pub fn spring(mut self, spring: Spring) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.spring = Some(spring);
        }
        self
    }

    /// Pins the current entry to a timing token, moving the cursor with it.
    pub fn at(mut self, token: &str) -> Self {
        let start = resolve_token(token, self.cursor, &self.labels);
        if let Some(entry) = self.entries.last_mut() {
            entry.start = start;
        }
        self.cursor = start;
        self
    }

    /// Registers every entry, delaying each until its instant arrives.
    pub fn play(self) {
        let Self {
            scheduler,
            entries,
            ..
        } = self;
        for entry in entries {
            let Some(node) = scheduler.arena.node_for_key(&entry.key) else {
                continue;
            };
            let from = MotionVector::from_state(&scheduler.arena.get(node).state.motion);
            let built = match entry.spring {
                Some(spring) => Controller::spring(node, from, entry.to, spring),
                None => Controller::tween(node, from, entry.to, entry.tween),
            };
            scheduler
                .motion
                .push(built.with_key(entry.key).with_delay(entry.start));
        }
    }
}
