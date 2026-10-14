// Single responsibility: A live handle to a mounted element, for reading the geometry it was given.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::foundation::snapshot::GeometrySnapshot;
use crate::foundation::{Point, ResolvedRect};
use crate::reactive::ReactiveRuntime;
use crate::tree::NodeId;

/// A handle to an element in the tree, created before that element exists.
///
/// The alternative to this is pushing measurements into signals: declare a sink, have layout
/// write it, subscribe, derive from the subscription. That is the right shape when a value
/// *changes* and something downstream has to follow — a row's height feeding its neighbour's
/// layout. It is the wrong shape for a client that simply wants to know where a thing is, because
/// it turns a read into a reaction: the reader has to predict that it will need the number,
/// subscribe in advance, and then invalidate whatever it derived. Reading geometry should not be
/// able to schedule a frame.
///
/// So this is a pull handle. `bind_ref` attaches it during reconciliation, when the node id is
/// finally known, and `rect` reads the frame's published geometry directly. Nothing here is
/// reactive, so asking a question cannot dirty a node, cannot feed back into the layout pass
/// that produced the answer, and cannot loop.
///
/// The trade is explicit and worth stating: because a read is not a notification, *nothing
/// re-runs when the geometry changes*. A caller that wants to react must sample — from a ticker,
/// or from a gesture, or after a layout it already knows happened. That is the correct default
/// (most geometry reads are one-off comparisons) but it is a real constraint, not a free win.
#[derive(Clone)]
pub struct ElementRef {
    /// The arena index of the bound element, once reconciliation has run.
    index: Rc<Cell<Option<u32>>>,
    /// Shared view of the runtime's geometry table. Cloning the view is a refcount bump, so
    /// every handle on a runtime reads the same frame.
    snapshot: GeometrySnapshot,
    /// Kept so the handle can register its interest at bind time, which it can only do through
    /// the runtime that owns the table.
    runtime: Rc<RefCell<ReactiveRuntime>>,
}

impl ElementRef {
    /// Creates an unbound handle on a runtime.
    pub fn new(runtime: Rc<RefCell<ReactiveRuntime>>) -> Self {
        // Read first, then move: the borrow has to end before the runtime can be moved in.
        let snapshot = runtime.borrow().snapshot();
        Self {
            index: Rc::new(Cell::new(None)),
            snapshot,
            runtime,
        }
    }

    /// Binds this handle to a node. Called by the reconciler, not by client code.
    pub(crate) fn bind(&self, node: NodeId) {
        self.index.set(Some(node.index));
        // Asked for once, at bind time, rather than on every read: the publish pass walks a set,
        // so a handle that is bound and rebuilt stays one entry.
        self.runtime.borrow_mut().watch_geometry(node.packed());
    }

    /// The runtime this handle reads from.
    pub fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        Rc::clone(&self.runtime)
    }

    /// Whether the handle has been bound to a live element yet.
    pub fn is_bound(&self) -> bool {
        self.index.get().is_some()
    }

    /// Where the element landed this frame, in window coordinates.
    ///
    /// `None` before the first layout of a frame, and `None` again for an element that was
    /// unmounted. Both are honest answers: the alternative — a zero rect — would let a caller
    /// measure a delta against nothing and animate out of the origin.
    pub fn rect(&self) -> Option<ResolvedRect> {
        self.snapshot.get(self.index.get()?)
    }

    /// Top-left of the element this frame, in window coordinates.
    pub fn position(&self) -> Option<Point> {
        self.rect().map(|r| Point::new(r.x, r.y))
    }

    /// Centre of the element this frame.
    pub fn center(&self) -> Option<Point> {
        self.rect()
            .map(|r| Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5))
    }

    /// Size of the element this frame.
    pub fn size(&self) -> Option<Point> {
        self.rect().map(|r| Point::new(r.width, r.height))
    }

    /// The boxes of this element's children this frame, in child order.
    ///
    /// The read side of a container's geometry, and the reason a list does not have to declare a
    /// pitch. Rows that fill a list share whatever space is left over, but each is clamped at its
    /// own min-content floor, so the gaps between them are neither a constant nor derivable from
    /// the list's height. Reading them settles the question instead of assuming an answer.
    pub fn child_rects(&self) -> Vec<ResolvedRect> {
        let Some(index) = self.index.get() else {
            return Vec::new();
        };
        self.snapshot
            .with_children(index, |children| children.to_vec())
            .unwrap_or_default()
    }
}

impl Default for ElementRef {
    fn default() -> Self {
        Self::new(Rc::new(RefCell::new(ReactiveRuntime::new())))
    }
}
