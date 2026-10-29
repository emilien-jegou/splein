// Single responsibility: The reorderable list container.
//
// Owns the `<for>` keyed by label (which is what lets each row's hook state —
// its FLIP above all — survive the rebuild a reorder causes) and the list
// handle the gesture measures through.

use std::rc::Rc;

use riode::prelude::*;

use super::drag::{Drag, RowSpec};
use super::row::RowProps;

/// The list, whose rows are rebuilt whenever the order signal changes.
///
/// Rows are keyed by label through `<for>`, which is what lets each row's hook
/// state — its FLIP above all — survive the rebuild. Keyed by position instead,
/// a reorder would hand each row the animation of whichever item moved into
/// its index.
pub fn list(drag: Rc<Drag>) -> impl IntoElement {
    let rows = drag.order;
    let items = rows.map({
        let drag = drag.clone();
        move |labels| {
            labels
                .into_iter()
                .map(|label| RowSpec::new(label, Rc::clone(&drag)))
                .collect::<Vec<_>>()
        }
    });
    let list_ref = drag.list.clone();

    view! {
        <Group
            key="list"
            direction="vertical"
            width="fill"
            height="fill"
            // No gap, no margins: each row tiles the list edge to edge, and the
            // spacing lives in the rows' own padding (see `PAD`), so it belongs
            // to the rows themselves.
            // Bound so the gesture can read the row boxes. Nothing is published into a signal: the
            // gesture reads them when the pointer moves, and a measurement that invalidated
            // subscribers would keep the redraw loop awake over a list that is not moving.
            ref={list_ref}
        >
            <for (i, spec) in items key={.label.clone()}>
                <Row index={i} spec={spec} />
            </for>
        </Group>
    }
}
