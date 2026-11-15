// Responsibility: Static/Dynamic ChildSegment lifecycle, reactive keyed lists, and flattening.

use super::GroupElement;
use crate::dsl::component::IntoElement;
use crate::dsl::element::{Element, IntoChildList};
use crate::reactive::readable::Readable;
use std::rc::Rc;


#[derive(Clone)]
pub enum ChildSegment {
    Static(Vec<Element>),
    Dynamic(Rc<dyn Fn() -> Vec<ChildSegment>>),
}

impl ChildSegment {
    pub fn keyed<S, R, K, F>(items: R, key_of: K, row: F) -> Self
    where
        R: IntoItems<S> + 'static,
        S: 'static,
        K: Fn(&S) -> String + 'static,
        F: Fn(usize, S) -> Vec<ChildSegment> + 'static,
    {
        Self::Dynamic(Rc::new(move || {
            items
                .into_items()
                .enumerate()
                .flat_map(|(i, s)| {
                    let key = key_of(&s);
                    match crate::reactive::Cx::current() {
                        Some(cx) => cx.enter_keyed(key, || row(i, s)),
                        None => row(i, s),
                    }
                })
                .collect()
        }))
    }

    pub fn keyed_index<S, R, F>(items: R, row: F) -> Self
    where
        R: IntoItems<S> + 'static,
        S: 'static,
        F: Fn(usize, S) -> Vec<ChildSegment> + 'static,
    {
        Self::Dynamic(Rc::new(move || {
            items
                .into_items()
                .enumerate()
                .flat_map(|(i, s)| match crate::reactive::Cx::current() {
                    Some(cx) => cx.enter_keyed(format!("index-{i}"), || row(i, s)),
                    None => row(i, s),
                })
                .collect()
        }))
    }
}

impl GroupElement {
    pub fn children<C: IntoChildList>(mut self, c: C) -> Self {
        self.child_segments
            .push(ChildSegment::Static(c.into_child_list()));
        self
    }
    pub fn child_segments(mut self, segs: Vec<ChildSegment>) -> Self {
        self.child_segments.extend(segs);
        self
    }
    pub fn has_dynamic_segments(&self) -> bool {
        self.child_segments
            .iter()
            .any(|s| matches!(s, ChildSegment::Dynamic(_)))
    }
    pub fn children_of(mut self, build: impl Fn() -> Vec<Element> + 'static) -> Self {
        self.child_segments
            .push(ChildSegment::Dynamic(Rc::new(move || {
                vec![ChildSegment::Static(build())]
            })));
        self
    }
    pub fn children_labels<S, R, F>(mut self, items: R, row: F) -> Self
    where
        R: Readable<Vec<S>> + 'static,
        S: Clone + 'static,
        F: Fn(usize, S) -> Element + 'static,
    {
        self.child_segments
            .push(ChildSegment::Dynamic(Rc::new(move || {
                vec![ChildSegment::Static(
                    items
                        .read()
                        .into_iter()
                        .enumerate()
                        .map(|(i, s)| row(i, s))
                        .collect(),
                )]
            })));
        self
    }
    pub fn children_labels_keyed<S, R, K, F>(mut self, items: R, key_of: K, row: F) -> Self
    where
        R: Readable<Vec<S>> + 'static,
        S: Clone + 'static,
        K: Fn(&S) -> String + 'static,
        F: Fn(usize, S) -> Element + 'static,
    {
        self.child_segments
            .push(ChildSegment::Dynamic(Rc::new(move || {
                vec![ChildSegment::Static(
                    items
                        .read()
                        .into_iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let key = key_of(&s);
                            match crate::reactive::Cx::current() {
                                Some(cx) => cx.enter_keyed(key.clone(), || {
                                    let el = row(i, s);
                                    let el = match el.key() {
                                        Some(_) => el,
                                        None => el.with_key(key.clone()),
                                    };
                                    if let Some(el_key) = el.key() {
                                        if let Some(scope) = crate::reactive::hooks::current_scope()
                                        {
                                            scope
                                                .runtime()
                                                .borrow_mut()
                                                .link_element_hook_instance(
                                                    el_key,
                                                    Rc::from(key.as_str()),
                                                );
                                        }
                                    }
                                    el
                                }),
                                None => row(i, s),
                            }
                        })
                        .collect(),
                )]
            })));
        self
    }
}

pub fn keyed_list<S, R, K, F>(items: R, key_of: K, row: F) -> Element
where
    R: Readable<Vec<S>> + 'static,
    S: Clone + 'static,
    K: Fn(&S) -> String + 'static,
    F: Fn(usize, S) -> Element + 'static,
{
    crate::dsl::group()
        .children_labels_keyed(items, key_of, row)
        .into_element()
}

pub fn index_keyed_list<S, R, F>(items: R, row: F) -> Element
where
    R: Readable<Vec<S>> + 'static,
    S: Clone + 'static,
    F: Fn(usize, S) -> Element + 'static,
{
    crate::dsl::group()
        .children_of(move || {
            items
                .read()
                .into_iter()
                .enumerate()
                .map(|(i, s)| match crate::reactive::Cx::current() {
                    Some(cx) => cx.enter_keyed(format!("index-{i}"), || row(i, s)),
                    None => row(i, s),
                })
                .collect()
        })
        .into_element()
}

pub fn flatten_segments(segments: &[ChildSegment]) -> Vec<Element> {
    let mut out = Vec::new();
    for seg in segments {
        match seg {
            ChildSegment::Static(els) => out.extend(els.iter().cloned()),
            ChildSegment::Dynamic(build) => out.extend(flatten_segments(&build())),
        }
    }
    out
}

/// A source of items for `<for>` loops and keyed lists.
/// Supports plain ranges (`0..24`), static collections (`Vec<T>`, arrays), and reactive signals.
pub trait IntoItems<T> {
    type Iter: Iterator<Item = T>;
    fn into_items(&self) -> Self::Iter;
}

// 1. Plain ranges: 0..24
impl<Idx: Clone> IntoItems<Idx> for std::ops::Range<Idx>
where
    std::ops::Range<Idx>: Iterator<Item = Idx> + Clone,
{
    type Iter = std::ops::Range<Idx>;
    fn into_items(&self) -> Self::Iter {
        self.clone()
    }
}

// 2. Inclusive ranges: 0..=24
impl<Idx: Clone> IntoItems<Idx> for std::ops::RangeInclusive<Idx>
where
    std::ops::RangeInclusive<Idx>: Iterator<Item = Idx> + Clone,
{
    type Iter = std::ops::RangeInclusive<Idx>;
    fn into_items(&self) -> Self::Iter {
        self.clone()
    }
}

// 3. Static Vec<T>
impl<T: Clone> IntoItems<T> for Vec<T> {
    type Iter = std::vec::IntoIter<T>;
    fn into_items(&self) -> Self::Iter {
        self.clone().into_iter()
    }
}

// 4. Fixed-size arrays: [T; N]
impl<T: Clone, const N: usize> IntoItems<T> for [T; N] {
    type Iter = std::array::IntoIter<T, N>;
    fn into_items(&self) -> Self::Iter {
        self.clone().into_iter()
    }
}

// 5. Reactive Signal<Vec<T>>
impl<T: Clone + 'static> IntoItems<T> for crate::reactive::Signal<Vec<T>> {
    type Iter = std::vec::IntoIter<T>;
    fn into_items(&self) -> Self::Iter {
        self.get().into_iter()
    }
}

// 6. Reactive Computed<Vec<T>>
impl<T: Clone + 'static> IntoItems<T> for crate::reactive::Computed<Vec<T>> {
    type Iter = std::vec::IntoIter<T>;
    fn into_items(&self) -> Self::Iter {
        self.get().into_iter()
    }
}
