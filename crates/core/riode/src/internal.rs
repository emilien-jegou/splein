//! Repr-builders: the macro's target language.
//!
//! These are the low-level constructors and types that `view!` expands into. They are
//! `pub` only so macro-generated code can reach them; they carry **no semver
//! guarantee** and will be renamed or refactored whenever the RSX surface needs it.
//!
//! # Do not import this directly
//!
//! Spell `<Group>` / `<Text>` / `<Image>` / `<Svg>` in `view!` instead — that is the
//! supported surface. This module exists for two deliberate escape hatches, each of
//! which should carry a comment explaining why RSX could not express the need:
//!
//! - **authoring a custom primitive** (`custom()` over a [`Primitive`](crate::tree::Primitive)),
//! - **generating RSX from code** (a codegen or macro that emits builders directly).
//!
//! The import is ugly on purpose: `use riode::internal::*;` at the top of one file.

pub use crate::dsl::{
    component_scope, component_scope_named, custom, group, image, index_keyed_list, keyed_list,
    svg, text, view_child, ChildSegment, CustomDef, Element, GroupDef, IntoElement, IntoSplice,
    TextDef,
};
