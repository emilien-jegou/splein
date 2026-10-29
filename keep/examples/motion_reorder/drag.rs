// Single responsibility: Shared list state and slot measurement.
//
// `Drag` is the one handle every row reads: the order list, the held row's
// identity, the grab offset, and the measured row boxes. (Despite the name it
// is list state, not the gesture — the press/track/release state machine is
// the engine's `riode::dsl::gesture::Drag`, bound per row in `row.rs`.)
// Rows never talk to each other directly; they all go through here.

use std::rc::Rc;

use riode::dsl::ElementRef;
use riode::foundation::{Point, ResolvedRect};
use riode::prelude::*;

/// The state the gesture needs, shared by every row.
///
/// Carried on the order signal's items rather than passed as list arguments, because
/// `children_labels_keyed` hands a row only `(index, item)` and threading a context through it
/// would mean changing the builder's shape. Keeping the payload on the item means the key is
/// still the label and the shared state rides along with it.
#[derive(Clone)]
pub struct RowSpec {
    pub label: String,
    pub drag: Rc<Drag>,
}

impl RowSpec {
    pub fn new(label: String, drag: Rc<Drag>) -> Self {
        Self { label, drag }
    }
}

pub struct Drag {
    pub order: Signal<Vec<String>>,
    pub revision: Signal<u32>,
    pub held: Signal<Option<String>>,
    pub grab_dy: Signal<f32>,
    /// Pointer intent with the grab offset folded in (`pointer.y - grab_dy`),
    /// so the held row keeps the grabbed point under the cursor instead of
    /// snapping its centre to it. Only the list can fold the offset (it alone
    /// measures slots), so this stays a list-owned signal even though the
    /// engine gesture tracks the raw position alongside it.
    pub want_centre: Signal<Option<f32>>,
    /// Live raw pointer position, owned by the engine gesture (bound per row
    /// in `row.rs`). Simple consumers drive motion straight from it; this
    /// list keeps `want_centre` instead (see above).
    pub gesture_at: Signal<Option<Point>>,
    /// Paint-order high-water mark (see `view.rs`): every grab takes the next
    /// level, so the most recent gesture always paints on top.
    pub z_top: Signal<i32>,
    /// Handle on the list, whose children *are* the rows.
    ///
    /// Reading the row boxes through the list rather than through each row's own handle is what
    /// lets one gesture compare the pointer against every slot without any of them cooperating.
    pub list: ElementRef,
}

impl Drag {
    /// Moves a row and bumps the revision each row's FLIP watches.
    pub fn move_row(&self, from: usize, to: usize) {
        if from == to {
            return;
        }
        self.order.update(|list| {
            let moved = list.remove(from);
            list.insert(to, moved);
        });
        self.revision.update(|r| *r += 1);
    }

    /// A row's current slot.
    pub fn slot_of(&self, label: &str) -> usize {
        self.order
            .get()
            .iter()
            .position(|l| l == label)
            .unwrap_or(0)
    }

    /// Every row's box this frame, in slot order.
    pub fn slots(&self) -> Vec<ResolvedRect> {
        self.list.child_rects().to_vec()
    }

    /// The slot the pointer is asking for, by measuring rather than by arithmetic.
    ///
    /// There is deliberately no pitch in this demo. Rows that fill a list share whatever space
    /// is left over, but each is then clamped at its own min-content floor, so a squashed window
    /// can leave the rows unequal — and then no single stride exists to divide a distance by.
    /// Reading the boxes sidesteps the question instead of answering it.
    pub fn target_slot(&self, centre: f32) -> Option<usize> {
        self.slots()
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = (a.y + a.height * 0.5 - centre).abs();
                let db = (b.y + b.height * 0.5 - centre).abs();
                da.total_cmp(&db)
            })
            .map(|(i, _)| i)
    }
}
