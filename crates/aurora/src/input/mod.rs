// Single responsibility: Pointer input: event vocabulary, painter-ordered hit testing, gestures.

pub mod event;
pub mod handlers;
pub mod hit;
pub mod keyboard;
pub mod router;
pub mod selection;
pub mod tracker;

pub use event::{ClickInfo, CursorIcon, DragInfo, HoverInfo, PointerButton, PointerEvent, PointerPhase};
pub use keyboard::{KeyCode, KeyEvent};
pub use handlers::{HandlerSet, NodeHandlers, Propagation};
pub use hit::{hit_chain, HitNode, HitTest};
pub use router::{PointerOutcome, PointerRouter};
pub use tracker::{PointerTracker, TrackerChange, DRAG_THRESHOLD};
