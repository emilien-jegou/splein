//! Pointer and keyboard input: from OS sample to handler call.
//!
//! Input travels a pipeline of its own: **hit** ([`hit::hit_chain`]) answers "what is on
//! top", **track** ([`PointerTracker`]) folds raw samples into gestures (press / drag /
//! release, with velocity and multi-click bursts), and **route**
//! ([`PointerRouter::dispatch`]) answers "who is owed the event" — press capture, hover
//! transitions, bubbling to ancestors. **Selection** (`selection`) owns the readonly
//! text-selection gestures on top.
//!
//! # The two rules that matter
//!
//! 1. **Release notifies the press owners; click needs the pointer.** `pointer_up` fires
//!    on whatever took the press, but `click` fires only on press-chain nodes still under
//!    the pointer (the web's common-ancestor rule). A drag released elsewhere never
//!    toggles what it started on; a click that merely wiggles still fires.
//! 2. **A drag only exists for draggable content.** If nobody in the press chain handles
//!    drags (and it's not selectable text), the press stays a press no matter how far it
//!    moves. This is what keeps an ordinary click that drifts past the threshold from
//!    dying silently.
//!
//! # Showcase
//!
//! Most apps never touch this module directly — handlers declared in the DSL are enough:
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! group()
//!     .width(200.0)
//!     .height(60.0)
//!     .fill(Color::hex(0x2563EB))
//!     .on_pointer_down(|e: &PointerEvent| { /* press at e.position */ })
//!     .on_click(|info: &ClickInfo| { /* info.click_count: 1, 2, 3… */ })
//!     .on_pointer_up(|_| {})
//!     .on_pointer_enter(|_| {})
//!     .on_pointer_leave(|_| {})
//! ```
//!
//! Drags need a handler to exist at all (rule 2). Keyboard arrives as [`KeyEvent`]
//! through [`PointerRouter::dispatch_key`]; text fields consume arrows/words/pages,
//! everything else bubbles like pointer events. [`Propagation::Stop`] halts a bubble;
//! the default lets it rise.
//!
//! # Reading input in tests
//!
//! The `Harness` (`aurora_tiny_skia::testing`) speaks this vocabulary for you:
//! `h.click("card")`, `h.drag_by("card", 100.0, 0.0, 8)` (releases at the end),
//! `h.drag_holding(..)` + `h.release_at()` (inspects the held state first), and raw
//! `h.pointer(point, phase)` for anything bespoke. See `tests/testing_harness.rs`.
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`event`] | [`PointerEvent`], [`PointerPhase`], [`PointerButton`], [`ClickInfo`], [`DragInfo`], [`HoverInfo`], [`CursorIcon`] |
//! | [`hit`] | [`hit_chain`], [`HitNode`], [`HitTest`] — painter-ordered resolution mirroring compile order, in reverse |
//! | [`tracker`] | [`PointerTracker`]: the gesture state machine (thresholds, velocity, bursts) |
//! | [`router`] | [`PointerRouter`]: dispatch, capture, hover, focus, keyboard routing |
//! | [`selection`] | Readonly text selection: press/extend/release, word snap, cross-node ranges, clipboard reads |
//! | [`handlers`] | [`HandlerSet`]/[`NodeHandlers`]: per-node closure storage + [`Propagation`] |
//! | [`keyboard`] | [`KeyEvent`], [`KeyCode`] |
//!
//! # Pitfalls
//!
//! - Pressing a node with no handlers and releasing 100 px away fires nothing —
//!   that is the common-ancestor rule working, not a swallowed event.
//! - `click_count` counts bursts *per target*: pressing elsewhere resets it, so rapid
//!   clicks on two different buttons never merge into a double-click.
//! - Selection drags bypass the draggable rule — selectable text is natively draggable,
//!   no handlers required.

pub mod event;
pub mod handlers;
pub mod hit;
pub mod keyboard;
pub mod router;
pub mod selection;
pub mod tracker;

pub use event::{
    ClickInfo, CursorIcon, DragInfo, HoverInfo, PointerButton, PointerEvent, PointerPhase,
};
pub use handlers::{HandlerSet, NodeHandlers, Propagation};
pub use hit::{hit_chain, HitNode, HitTest};
pub use keyboard::{KeyCode, KeyEvent};
pub use router::{PointerOutcome, PointerRouter};
pub use tracker::{PointerTracker, TrackerChange, DRAG_THRESHOLD};
