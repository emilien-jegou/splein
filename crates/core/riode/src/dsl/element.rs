// Single responsibility: Ephemeral element sum type and recursive child list conversions.

pub use crate::dsl::component::IntoElement;
use crate::dsl::custom::CustomDef;
use crate::dsl::group::GroupElement;
use crate::dsl::image::ImageElement;
use crate::dsl::text::TextElement;
use crate::foundation::Key;

#[derive(Clone)]
pub enum Element {
    Group(GroupElement),
    Text(TextElement),
    Image(ImageElement),
    Custom(CustomDef),
}

impl Element {
    pub fn key(&self) -> Option<Key> {
        match self {
            Self::Group(g) => g.key.clone(),
            Self::Text(t) => t.key.clone(),
            Self::Image(i) => i.key.clone(),
            Self::Custom(c) => c.key.clone(),
        }
    }

    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        let key = Some(key.into());
        match &mut self {
            Self::Group(g) => g.key = key,
            Self::Text(t) => t.key = key,
            Self::Image(i) => i.key = key,
            Self::Custom(c) => c.key = key,
        }
        self
    }

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
                    "Group key={:?} material={} w={} h={}\n",
                    g.key,
                    prop_summary(&g.material),
                    prop_summary(&g.width),
                    prop_summary(&g.height),
                ));
                if depth >= MAX_DEPTH {
                    for _ in 0..=depth { out.push_str("  "); }
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
                            for _ in 0..=depth { out.push_str("  "); }
                            out.push_str("<dynamic children>\n");
                        }
                    }
                }
                if total > shown {
                    for _ in 0..=depth { out.push_str("  "); }
                    out.push_str(&format!("…({} more)\n", total - shown));
                }
            }
            Element::Text(t) => {
                let content = match &t.content {
                    crate::reactive::prop::Prop::Static(s) => {
                        let mut short: String = s.chars().take(24).collect();
                        if s.len() > short.len() { short.push('…'); }
                        format!("{short:?}")
                    }
                    crate::reactive::prop::Prop::Dynamic(_) => "<dyn>".to_string(),
                    crate::reactive::prop::Prop::Context(_) => "<ctx>".to_string(),
                };
                out.push_str(&format!("Text key={:?} content={content} color={:?} size={}\n", t.key, t.color, t.size));
            }
            Element::Image(i) => {
                out.push_str(&format!("Image key={:?} fit={:?} ({}x{})\n", i.key, i.fit, i.source.width, i.source.height));
            }
            Element::Custom(c) => {
                out.push_str(&format!("Custom key={:?}\n", c.key));
            }
        }
    }
}

fn prop_summary<T: std::fmt::Debug>(prop: &Option<crate::reactive::prop::Prop<T>>) -> String {
    match prop {
        None => "-".to_string(),
        Some(crate::reactive::prop::Prop::Static(v)) => format!("{v:?}"),
        Some(crate::reactive::prop::Prop::Dynamic(_)) => "<dyn>".to_string(),
        Some(crate::reactive::prop::Prop::Context(_)) => "<ctx>".to_string(),
    }
}

impl From<GroupElement> for Element {
    fn from(def: GroupElement) -> Self { Element::Group(def) }
}
impl From<TextElement> for Element {
    fn from(def: TextElement) -> Self { Element::Text(def) }
}
impl From<ImageElement> for Element {
    fn from(def: ImageElement) -> Self { Element::Image(def) }
}
impl From<CustomDef> for Element {
    fn from(def: CustomDef) -> Self { Element::Custom(def) }
}

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
