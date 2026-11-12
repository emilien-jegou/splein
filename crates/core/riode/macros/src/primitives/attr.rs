// Single responsibility: Riode-specific attribute normalization (Tailwind, shorthands) and value wrapping.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rstml::node::{KVAttributeValue, KeyedAttribute, KeyedAttributeValue};
use crate::engine::flow::FlowTables;

pub fn attr_method(tag: &str, key: &str) -> String {
    if key == "ref" { return "bind_ref".to_string(); }
    if let Some(ev) = key.strip_prefix("on:") {
        return format!("on_{}", ev.replace(['-', ':'], "_"));
    }
    let normalized = match key {
        "w" if tag != "text" => "w",
        "h" => "h",
        "z" => "z",
        "ratio" => "ratio",
        "align" if tag != "text" => "align",
        "align_h" => "align_h",
        "align_v" => "align_v",
        "p" | "px" | "py" | "pt" | "pr" | "pb" | "pl" => key,
        "m" | "mx" | "my" | "mt" | "mr" | "mb" | "ml" => key,
        "s" if tag == "text" => "s",
        "w" if tag == "text" => "w",
        "align" if tag == "text" => "align",
        "show" => "show",
        "hide" => "hide",
        other => other,
    };
    normalized.replace(['-', ':'], "_")
}

pub fn slot_literal_value(val: TokenStream2, flow: &FlowTables) -> TokenStream2 {
    if let Ok(s) = syn::parse2::<syn::LitStr>(val.clone()) {
        let idx = flow.slots.get();
        flow.slots.set(idx + 1);
        return quote! { ::riode::hot_reload::template::tpl_string(::std::concat!(::std::file!(), ":", ::std::line!(), ":", #idx), #s) };
    }
    if let Ok(f) = syn::parse2::<syn::LitFloat>(val.clone()) {
        let repr = f.to_string();
        if repr.contains('.') {
            let idx = flow.slots.get();
            flow.slots.set(idx + 1);
            return quote! { ::riode::hot_reload::template::tpl_f32(::std::concat!(::std::file!(), ":", ::std::line!(), ":", #idx), #f) };
        }
    }
    val
}

pub fn attr_value_tokens(attr: &KeyedAttribute) -> TokenStream2 {
    match &attr.possible_value {
        KeyedAttributeValue::Binding(_) => quote! { ::std::compile_error!("view!: closure bindings not supported") },
        KeyedAttributeValue::Value(v) => match &v.value {
            KVAttributeValue::Expr(syn::Expr::Block(b)) if b.attrs.is_empty() && b.label.is_none() => strip_braces(&b.block),
            KVAttributeValue::Expr(e) => quote! { #e },
            KVAttributeValue::InvalidBraced(b) => crate::engine::nested_element_value(&b.body, &crate::primitives::riode_engine()),
        },
        KeyedAttributeValue::None => quote! { true },
    }
}

pub fn strip_braces(block: &syn::Block) -> TokenStream2 {
    use quote::ToTokens;
    if block.stmts.len() == 1 {
        if let Some(syn::Stmt::Expr(e, None)) = block.stmts.first() {
            return e.to_token_stream();
        }
    }
    block.to_token_stream()
}

pub fn attr_value(key: &str, attr: &KeyedAttribute, flow: &FlowTables) -> TokenStream2 {
    if key.starts_with("on:") {
        if let Some(syn::Expr::Field(syn::ExprField { base, member, .. })) = attr_expr(attr) {
            return quote! { move |_| (#base).#member() };
        }
    }
    match key {
        "material" => {
            if let Some(list) = bracket_list(attr, &quote!(::riode::foundation::Material), &quote!(::riode::foundation::Stage)) {
                return slot_literal_value(list, flow);
            }
            if let Some(syn::Expr::Closure(_)) = attr_expr(attr) {
                let inner = attr_value_tokens(attr);
                return slot_literal_value(quote! { ::riode::foundation::GeoFn(#inner) }, flow);
            }
            slot_literal_value(attr_value_tokens(attr), flow)
        }
        "overlay" => {
            if let Some(list) = bracket_list(attr, &quote!(::riode::foundation::Overlay), &quote!(::riode::foundation::Stage)) {
                return slot_literal_value(list, flow);
            }
            slot_literal_value(attr_value_tokens(attr), flow)
        }
        "effect" => {
            if let Some(list) = bracket_list(attr, &quote!(::riode::foundation::Effect), &quote!(::riode::foundation::EffectStage)) {
                return slot_literal_value(list, flow);
            }
            slot_literal_value(attr_value_tokens(attr), flow)
        }
        _ => slot_literal_value(attr_value_tokens(attr), flow),
    }
}

fn attr_expr(attr: &KeyedAttribute) -> Option<syn::Expr> {
    match &attr.possible_value {
        KeyedAttributeValue::Value(v) => match &v.value {
            KVAttributeValue::Expr(syn::Expr::Block(b)) if b.attrs.is_empty() && b.label.is_none() => {
                if b.block.stmts.len() == 1 {
                    if let Some(syn::Stmt::Expr(e, None)) = b.block.stmts.first() {
                        return Some(e.clone());
                    }
                }
                Some(syn::Expr::Block(b.clone()))
            }
            KVAttributeValue::Expr(e) => Some(e.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn bracket_list(attr: &KeyedAttribute, ty: &TokenStream2, tr: &TokenStream2) -> Option<TokenStream2> {
    let syn::Expr::Array(arr) = attr_expr(attr)? else { return None };
    if !arr.attrs.is_empty() { return None; }
    if arr.elems.is_empty() { return Some(quote! { #ty::empty() }); }
    let items = arr.elems.into_iter();
    Some(quote! { #ty::from_boxes(::std::vec![#(::std::boxed::Box::new(#items) as ::std::boxed::Box<dyn #tr>),*]) })
}
