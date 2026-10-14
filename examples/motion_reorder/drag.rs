// Single responsibility: Shared drag gesture state and slot measurement.
//
// `Drag` is the one handle every row reads: the order list, the held row's
// identity, the pointer's intent, and the measured row boxes. Rows never talk
// to each other directly; they all go through here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aurora::dsl::ElementRef;
use aurora::foundation::{ResolvedRect, Transform};
use aurora::prelude::*;

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
    pub want_centre: Signal<Option<f32>>,
    /// Handle on the list, whose children *are* the rows.
    ///
    /// Reading the row boxes through the list rather than through each row's own handle is what
    /// lets one gesture compare the pointer against every slot without any of them cooperating.
    pub list: ElementRef,
    /// Every row's release-glide state, by label. A new grab cancels all stale
    /// glides (see `cancel_stale_glides`): a glide left running from a previous
    /// drag keeps its row's FLIP masked, and new swaps re-invert the masked
    /// spring forever, so the row teleports every swap until something clears it.
    pub settlers: RefCell<HashMap<String, (Signal<bool>, Signal<Transform>)>>,
}

impl Drag {
    /// Cancels every row's release-glide unconditionally.
    ///
    /// A fresh gesture obsoletes all release motion: whatever was gliding home is
    /// either already home (no-op) or about to be dragged again (stale offset that
    /// would fight the pointer). Critically, a glide whose FLIP never jointly
    /// finished keeps masking that row's FLIP, and each new swap re-inverts the
    /// masked spring — teleporting every move for the whole drag. Grab time is
    /// the one moment all rows are accounted for, so this runs first, here.
    pub fn cancel_stale_glides(&self) {
        for (_, (settling, settle)) in self.settlers.borrow().iter() {
            if settling.get() {
                settling.set(false);
                settle.set(Transform::IDENTITY);
            }
        }
    }

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
