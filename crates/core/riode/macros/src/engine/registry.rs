// Single responsibility: Declarative registry of RSX primitives, capability facets, and surface props.

use super::ast::Element;
use super::flow::FlowTables;
use proc_macro2::TokenStream as TokenStream2;
use std::collections::HashMap;

pub struct EngineContext<'a> {
    pub el: &'a Element,
    pub flow: &'a FlowTables,
    pub engine: &'a RxEngine,
}

pub type ExpandFn = fn(EngineContext) -> syn::Result<TokenStream2>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Facet {
    Geometry,
    Spacing,
    Position,
    Visibility,
}

#[derive(Clone)]
pub struct SurfacePropDef {
    pub name: &'static str,
    pub facet: Facet,
    pub arg_type: TokenStream2,
    pub apply: TokenStream2,
}

#[derive(Clone)]
pub struct PrimitiveDef {
    pub tag: &'static str,
    pub aliases: &'static [&'static str],
    pub type_names: &'static [&'static str],
    pub expand: ExpandFn,
    pub supported_facets: Vec<Facet>,
}

#[derive(Default)]
pub struct RxEngine {
    primitives: HashMap<&'static str, PrimitiveDef>,
    surface_props: Vec<SurfacePropDef>,
}

impl RxEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_primitive(mut self, def: PrimitiveDef) -> Self {
        self.primitives.insert(def.tag, def.clone());
        for &alias in def.aliases {
            self.primitives.insert(alias, def.clone());
        }
        self
    }

    pub fn register_surface_prop(mut self, prop: SurfacePropDef) -> Self {
        self.surface_props.push(prop);
        self
    }

    pub fn get(&self, tag: &str) -> Option<&PrimitiveDef> {
        self.primitives.get(tag)
    }

    pub fn tag_for_type_name(&self, type_name: &str) -> Option<&'static str> {
        self.primitives
            .values()
            .find(|def| def.type_names.contains(&type_name))
            .map(|def| def.tag)
    }

    pub fn surface_props_for(&self, root_tag: &str) -> Vec<&SurfacePropDef> {
        let Some(prim) = self.get(root_tag) else {
            return Vec::new();
        };
        self.surface_props
            .iter()
            .filter(|p| prim.supported_facets.contains(&p.facet))
            .collect()
    }
}
