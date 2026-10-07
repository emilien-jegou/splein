// Single responsibility: Pointer input: event vocabulary, painter-ordered hit testing, gestures.

pub mod event;
pub mod handlers;
pub mod hit;
pub mod router;
pub mod tracker;

pub use event::{ClickInfo, DragInfo, HoverInfo, PointerButton, PointerEvent, PointerPhase};
pub use handlers::{HandlerSet, NodeHandlers, Propagation};
pub use hit::{hit_chain, HitNode, HitTest};
pub use router::{PointerOutcome, PointerRouter};
pub use tracker::{PointerTracker, TrackerChange, DRAG_THRESHOLD};
