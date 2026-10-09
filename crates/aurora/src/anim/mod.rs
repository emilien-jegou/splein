//! Time-based interpolation, independent of the UI tree.
//!
//! This module answers one question: *given a target that moves, what value should we show
//! right now?* Everything here is pure math over seconds — no nodes, no layout, no pixels.
//! The reactive layer ([`crate::reactive`]) feeds these curves time; the tree binds their
//! outputs to whatever should move.
//!
//! There are two flavors of motion, and picking the wrong one is the most common animation
//! bug in this engine:
//!
//! - **Timed** ([`Tween`] + [`Ease`]): "get there in exactly N seconds along this curve."
//!   Use it for choreography — fades, entrances, anything with a deadline.
//! - **Physics** ([`Spring`], [`track::ScalarTrack`]): "settle toward the target like a
//!   physical spring." Use it for anything the user can interrupt mid-flight — drags,
//!   toggles, FLIP reorder — because a spring adopts live position *and* velocity when
//!   retargeted, so grabbing a moving card never teleports it.
//!
//! [`Transition`] is the adapter that binds either flavor to a signal: the target says
//! *where to go*, the transition decides *how*, and the frame loop supplies the time.
//!
//! # Showcase
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! #[component]
//! fn SlidingPanel(open: Signal<bool>) -> impl IntoElement {
//!     // One target, two personalities: the panel eases open on a deadline…
//!     let ease = Transition::ease(0.35, Ease::OutCubic);
//!     let x_ease = ease.on(open.map(|o| if o { 0.0 } else { -320.0 }));
//!     // …while the handle follows the pointer with spring physics.
//!     let spring = Transition::spring(Spring::snappy());
//!     let x_spring = spring.on(open.map(|o| if o { 0.0 } else { -320.0 }));
//!     group().width(320.0).height(480.0).x(x_ease).children([
//!         group().width(24.0).height(480.0).x(x_spring),
//!     ])
//! }
//! ```
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`clock`] | [`Clock`]: the single frame clock, fixed 120 Hz quantum stepping |
//! | [`ease`] | [`Ease`]: named easing curves (linear, cubic, quint, expo) |
//! | [`spring`] | [`Spring`]/[`SpringState`]: spring parameters + substepped simulation |
//! | [`track`] | [`track::ScalarTrack`]: stateful settling toward a moving target |
//! | [`transition`] | [`Transition`], [`transition::TransitionCell`], [`ControllerHandle`]: signal adapters + playback control |
//! | [`tween`] | [`Tween`], [`tween::Millis`]: duration-bounded interpolation |
//!
//! # Pitfalls
//!
//! - A tween replays from its start on every retarget; a spring continues from where it
//!   is. If your animation "jumps" when the target changes mid-flight, you wanted a spring.
//! - [`ControllerHandle`] (pause/resume/restart/seek) only exists for playbacks created
//!   with [`Transition::playback`] / [`crate::reactive::use_playback`] — plain `.on()`
//!   transitions are fully driven by their target signal and need no handle.

pub mod clock;
pub mod ease;
pub mod spring;
pub mod track;
pub mod transition;
pub mod tween;

pub use clock::Clock;
pub use ease::Ease;
pub use spring::{Spring, SpringState};
pub use track::{RectTrack, RectVector, ScalarTrack};
pub use transition::{ControllerHandle, PlayState, Repeat, Transition};
pub use tween::{Millis, Tween};
