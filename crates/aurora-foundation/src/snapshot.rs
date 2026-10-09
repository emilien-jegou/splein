// Single responsibility: A shared, read-on-demand table of the geometry layout just produced.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::geometry::{Point, ResolvedRect};

/// Where the elements that asked to know ended up this frame, readable on demand.
///
/// Cheap to clone: every clone is another view of the same table, not a copy of it. That is what
/// lets a handle hold one without owning the frame's geometry.
///
/// The alternative is pushing each measurement into a signal the client subscribes to. That is
/// what `measure_into` does and it works, but it makes reading geometry into a *reaction*: the
/// client has to have predicted that it would need the number, subscribe to it, and then
/// invalidate whatever derived from it. A client that only wants to know where a thing is — to
/// compare it against a pointer, to measure a delta, to decide a hit — should be able to ask.
///
/// This is the pull counterpart. Layout writes the table once per frame, after placement and
/// before anything can read it, and a handle reads it directly. Nothing here is reactive, so
/// reading a position cannot mark a node dirty, cannot schedule a frame, and cannot feed back
/// into the layout pass that produced it.
///
/// Only elements that asked to be here are written, so the cost is proportional to the number of
/// elements doing geometry rather than to the size of the tree.
///
/// Keys are raw arena indices rather than node ids: this sits in the foundation vocabulary so a
/// handle can be built without the handle itself needing to know what a node is.
#[derive(Clone, Default)]
pub struct GeometrySnapshot {
    boxes: Rc<RefCell<HashMap<u32, MeasuredElement>>>,
}

/// One element's measurement: its own box, and the boxes of its children in child order.
///
/// Both are needed because the two questions a client asks are different. "Where am I" is the
/// element's own box. "What slots do my children occupy" is the children's boxes in the order
/// they were built in, which is the only way to learn a pitch or a gap without assuming one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeasuredElement {
    /// The element's own box, in window coordinates.
    pub rect: ResolvedRect,
    /// The element's children, in child order, in window coordinates.
    pub children: Box<[ResolvedRect]>,
}

impl GeometrySnapshot {
    /// Creates an empty snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets last frame's measurements.
    ///
    /// Called after layout has run and before anything reads, which is what makes `get` mean
    /// "measured this frame": a handle read before the first layout of the frame gets `None`
    /// rather than a stale answer from whenever the element last moved.
    pub fn clear(&self) {
        self.boxes.borrow_mut().clear();
    }

    /// Records where an element landed, and where its children landed.
    pub fn put(&self, index: u32, measured: MeasuredElement) {
        self.boxes.borrow_mut().insert(index, measured);
    }

    /// Index-keyed access used by the arena, which knows nodes by id rather than by raw index.

    /// Where an element landed, or `None` if it was not measured this frame.
    ///
    /// Distinguishing "at the origin" from "no idea" matters: a client measuring a delta against
    /// a default-constructed rect would animate a long way out of nowhere the first time an
    /// element appeared.
    pub fn get(&self, index: u32) -> Option<ResolvedRect> {
        self.boxes.borrow().get(&index).map(|m| m.rect)
    }

    /// The boxes of an element's children this frame, in child order.
    ///
    /// The read side of a list's geometry. A client that needs to know how far apart two rows
    /// are, or which row a pointer is over, has to measure rather than assume: rows that fill a
    /// list share whatever space is left over, but each is clamped at its own min-content floor,
    /// so the gaps between them are not a constant and not derivable from the list's height.
    /// Borrowed through a closure rather than returned by reference: handing out a `&[ResolvedRect]`
    /// into a `RefCell` would return a pointer the next `put` could invalidate underneath the
    /// caller, mid-read.
    ///
    /// The closure keeps the borrow for the length of the read, which costs one map lookup —
    /// copying instead would cost one clone per row, per drag step.
    pub fn with_children<R>(
        &self,
        index: u32,
        read: impl FnOnce(&[ResolvedRect]) -> R,
    ) -> Option<R> {
        let boxes = self.boxes.borrow();
        boxes.get(&index).map(|m| read(&m.children))
    }

    /// Top-left of an element in window coordinates.
    pub fn position(&self, index: u32) -> Option<Point> {
        self.get(index).map(|r| Point::new(r.x, r.y))
    }

    /// Number of elements measured this frame. Diagnostics and tests only.
    pub fn len(&self) -> usize {
        self.boxes.borrow().len()
    }

    /// Whether nothing was measured this frame.
    pub fn is_empty(&self) -> bool {
        self.boxes.borrow().is_empty()
    }
}
