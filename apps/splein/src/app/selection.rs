// Models figma-style selection boxes, resize rects, and pointer-press routing.

use super::gesture::GestureEngine;
use crate::domain::canvas::{Canvas, DrawingElement};
use crate::domain::geometry::{Aabb, Handle, Vec2};

/// Selection rect: element bounds padded by half its stroke width.
pub fn selection_rect(elem: &DrawingElement) -> Aabb {
    let b = elem.aabb();
    let pad = elem.stroke_padding();
    Aabb::new(
        Vec2::new(b.min.x - pad, b.min.y - pad),
        Vec2::new(b.max.x + pad, b.max.y + pad),
    )
}

/// Union of the selection rects of every selected element.
pub fn group_rect(canvas: &Canvas, selected: &[usize]) -> Option<Aabb> {
    selected
        .iter()
        .filter_map(|&idx| canvas.element(idx))
        .map(selection_rect)
        .reduce(|mut group, rect| {
            group.union(rect);
            group
        })
}

/// Grows or shrinks `base` toward `pos`, keeping the side opposite `handle` fixed.
pub fn resize_rect(base: Aabb, handle: Handle, pos: Vec2) -> Aabb {
    let (mut l, mut t) = (base.min.x, base.min.y);
    let (mut r, mut b) = (base.max.x, base.max.y);
    match handle {
        Handle::Nw => {
            l = pos.x.min(r - 1.0);
            t = pos.y.min(b - 1.0);
        }
        Handle::Ne => {
            r = pos.x.max(l + 1.0);
            t = pos.y.min(b - 1.0);
        }
        Handle::Sw => {
            l = pos.x.min(r - 1.0);
            b = pos.y.max(t + 1.0);
        }
        Handle::Se => {
            r = pos.x.max(l + 1.0);
            b = pos.y.max(t + 1.0);
        }
        Handle::N => t = pos.y.min(b - 1.0),
        Handle::S => b = pos.y.max(t + 1.0),
        Handle::W => l = pos.x.min(r - 1.0),
        Handle::E => r = pos.x.max(l + 1.0),
    }
    Aabb::new(Vec2::new(l, t), Vec2::new(r, b))
}

/// Routes a pointer-tool press: resize, move selection, select one element, or band.
pub fn on_pointer_press(
    canvas: &mut Canvas,
    gestures: &mut GestureEngine,
    selected: &[usize],
    pos: Vec2,
) -> Vec<usize> {
    if let Some(group) = group_rect(canvas, selected) {
        if let Some(handle) = Handle::hit(pos, group) {
            gestures.begin_resize(selected.to_vec(), group, handle, pos);
            return selected.to_vec();
        }
        let inside_selection = selected
            .iter()
            .filter_map(|&idx| canvas.element(idx))
            .any(|elem| selection_rect(elem).contains_with_padding(pos, 0.0));
        if inside_selection {
            gestures.begin_move(selected.to_vec(), pos);
            return selected.to_vec();
        }
    }
    if let Some(idx) = canvas.hit_test(pos, 14.0) {
        gestures.begin_move(vec![idx], pos);
        return vec![idx];
    }
    gestures.begin_band(pos);
    Vec::new()
}
