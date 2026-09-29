// Single responsibility: Ephemeral element sum type and recursive child list conversions.

use crate::dsl::component::IntoElement;
use crate::dsl::custom::CustomDef;
use crate::dsl::group::GroupDef;
use crate::dsl::text::TextDef;
use crate::foundation::Key;

/// Ephemeral element enum representing declarative UI tree specifications.
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
}

impl From<GroupDef> for Element {
    fn from(def: GroupDef) -> Self { Element::Group(def) }
}

impl From<TextDef> for Element {
    fn from(def: TextDef) -> Self { Element::Text(def) }
}

impl From<CustomDef> for Element {
    fn from(def: CustomDef) -> Self { Element::Custom(def) }
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
