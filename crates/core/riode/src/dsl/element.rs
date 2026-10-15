// Single responsibility: Ephemeral element sum type and recursive child list conversions.

use crate::dsl::component::IntoElement;
use crate::dsl::custom::CustomDef;
use crate::dsl::group::GroupDef;
use crate::dsl::text::TextDef;
use crate::foundation::Key;

/// Ephemeral element enum representing declarative UI tree specifications.
#[derive(Clone)]
pub enum Element {
    Group(GroupDef),
    Text(TextDef),
    Custom(CustomDef),
}

impl Element {
    pub fn key(&self) -> Option<Key> {
        match self {
            Self::Group(g) => g.key.clone(),
            Self::Text(t) => t.key.clone(),
            // FIX: previously always None, forcing positional diffing for all
            // custom/SVG children.
            Self::Custom(c) => c.key.clone(),
        }
    }

    /// Returns this element with its reconciliation key set.
    ///
    /// The generated component builder applies it to the element the body
    /// returned: the call site owns identity (`<Tag key={…}>`), so this
    /// overwrites any key the body declared on its own root.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        let key = Some(key.into());
        match &mut self {
            Self::Group(g) => g.key = key,
            Self::Text(t) => t.key = key,
            Self::Custom(c) => c.key = key,
        }
        self
    }

    /// Whether reconciling this element may read context (and therefore miss
    /// a provider). Reconcile wraps such children so a missing provider
    /// mounts an empty placeholder plus a console error instead of aborting the walk.
    pub fn has_context_reads(&self) -> bool {
        match self {
            Self::Group(g) => g.has_context_reads(),
            Self::Text(t) => t.has_context_reads(),
            Self::Custom(_) => false,
        }
    }

    /// One-line-per-node debug summary of the ephemeral tree: the reload IR made
    /// human-diffable. Static values render; dynamic/context props render as
    /// `<dyn>`/`<ctx>` (their closures are opaque); custom primitives render by
    /// key only. Capped depth and breadth — diagnostics, not serialization.
    pub fn summarize(&self) -> String {
        let mut out = String::new();
        Self::summarize_into(self, 0, &mut out);
        out
    }

    fn summarize_into(el: &Element, depth: usize, out: &mut String) {
        const MAX_DEPTH: usize = 6;
        const MAX_CHILDREN: usize = 8;
        for _ in 0..depth {
            out.push_str("  ");
        }
        match el {
            Element::Group(g) => {
                out.push_str(&format!(
                    "Group key={:?} fill={} w={} h={}\n",
                    g.key,
                    prop_summary(&g.fill),
                    prop_summary(&g.width),
                    prop_summary(&g.height),
                ));
                if depth >= MAX_DEPTH {
                    for _ in 0..=depth {
                        out.push_str("  ");
                    }
                    out.push_str("…\n");
                    return;
                }
                let mut shown = 0;
                let mut total = 0;
                for seg in &g.child_segments {
                    match seg {
                        crate::dsl::group::ChildSegment::Static(els) => {
                            for child in els {
                                total += 1;
                                if shown < MAX_CHILDREN {
                                    shown += 1;
                                    Self::summarize_into(child, depth + 1, out);
                                }
                            }
                        }
                        crate::dsl::group::ChildSegment::Dynamic(_) => {
                            for _ in 0..=depth {
                                out.push_str("  ");
                            }
                            out.push_str("<dynamic children>\n");
                        }
                    }
                }
                if total > shown {
                    for _ in 0..=depth {
                        out.push_str("  ");
                    }
                    out.push_str(&format!("…({} more)\n", total - shown));
                }
            }
            Element::Text(t) => {
                let content = match &t.content {
                    crate::reactive::prop::DynamicProp::Static(s) => {
                        let mut short: String = s.chars().take(24).collect();
                        if s.len() > short.len() {
                            short.push('…');
                        }
                        format!("{short:?}")
                    }
                    crate::reactive::prop::DynamicProp::Dynamic(_) => "<dyn>".to_string(),
                    crate::reactive::prop::DynamicProp::Context(_) => "<ctx>".to_string(),
                };
                out.push_str(&format!(
                    "Text key={:?} content={content} color={:?} size={}\n",
                    t.key, t.color, t.size,
                ));
            }
            Element::Custom(c) => {
                out.push_str(&format!("Custom key={:?}\n", c.key));
            }
        }
    }
}

/// Renders one optional reactive prop for [`Element::summarize`]: statics by value,
/// dynamics and context reads by kind, absence as `-`.
fn prop_summary<T: std::fmt::Debug>(
    prop: &Option<crate::reactive::prop::DynamicProp<T>>,
) -> String {
    match prop {
        None => "-".to_string(),
        Some(crate::reactive::prop::DynamicProp::Static(v)) => format!("{v:?}"),
        Some(crate::reactive::prop::DynamicProp::Dynamic(_)) => "<dyn>".to_string(),
        Some(crate::reactive::prop::DynamicProp::Context(_)) => "<ctx>".to_string(),
    }
}

impl From<GroupDef> for Element {
    fn from(def: GroupDef) -> Self {
        Element::Group(def)
    }
}

impl From<TextDef> for Element {
    fn from(def: TextDef) -> Self {
        Element::Text(def)
    }
}

impl From<CustomDef> for Element {
    fn from(def: CustomDef) -> Self {
        Element::Custom(def)
    }
}

/// Trait converting collections of elements into a flat child vector.
pub trait IntoChildList {
    fn into_child_list(self) -> Vec<Element>;
}

impl<T: IntoElement> IntoChildList for Vec<T> {
    fn into_child_list(self) -> Vec<Element> {
        self.into_iter().map(|item| item.into_element()).collect()
    }
}

impl<T: IntoElement, const N: usize> IntoChildList for [T; N] {
    fn into_child_list(self) -> Vec<Element> {
        self.into_iter().map(|item| item.into_element()).collect()
    }
}

macro_rules! impl_into_child_list_tuple {
    ($($T:ident),+) => {
        impl<$($T: IntoElement),+> IntoChildList for ($($T,)+) {
            #[allow(non_snake_case)]
            fn into_child_list(self) -> Vec<Element> {
                let ($($T,)+) = self;
                vec![$($T.into_element()),+]
            }
        }
    };
}

impl_into_child_list_tuple!(A, B);
impl_into_child_list_tuple!(A, B, C);
impl_into_child_list_tuple!(A, B, C, D);
impl_into_child_list_tuple!(A, B, C, D, E);
impl_into_child_list_tuple!(A, B, C, D, E, F);
impl_into_child_list_tuple!(A, B, C, D, E, F, G);
impl_into_child_list_tuple!(A, B, C, D, E, F, G, H);
impl_into_child_list_tuple!(A, B, C, D, E, F, G, H, I);
impl_into_child_list_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_into_child_list_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_into_child_list_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{group, text};
    use crate::foundation::Color;

    /// The reload IR summary renders static values for human diffing.
    #[test]
    fn summarize_renders_statics_for_diffing() {
        let tree = group()
            .width(500.0)
            .fill(Color::WHITE)
            .children([text("Hello").size(16.0).color(Color::BLACK)])
            .into_element();
        let summary = tree.summarize();
        assert!(
            summary.contains("Group") && summary.contains("Text"),
            "both nodes present:\n{summary}"
        );
        assert!(
            summary.contains("\"Hello\""),
            "static content rendered:\n{summary}"
        );
        assert!(
            summary.contains("Black") || summary.contains("0.0, g: 0.0, b: 0.0"),
            "static color rendered:\n{summary}"
        );
    }
}
