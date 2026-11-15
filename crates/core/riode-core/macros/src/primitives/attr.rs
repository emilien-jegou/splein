// Responsibility: Riode-specific attribute normalization (Tailwind, shorthands) and value wrapping.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use rstml::node::{KVAttributeValue, KeyedAttribute, KeyedAttributeValue};

pub fn attr_method(key: &str) -> String {
    if key == "ref" {
        return "bind_ref".to_string();
    }
    if let Some(ev) = key.strip_prefix("on:") {
        return format!("on_{}", ev.replace(['-', ':'], "_"));
    }
    let normalized = match key {
        "lh" | "line-height" => "line_height",
        other => other,
    };
    normalized.replace(['-', ':'], "_")
}

pub fn attr_value_tokens(attr: &KeyedAttribute) -> TokenStream2 {
    match &attr.possible_value {
        KeyedAttributeValue::Binding(_) => quote! { ::std::compile_error!("view!: closure bindings not supported") },
        KeyedAttributeValue::Value(v) => match &v.value {
            KVAttributeValue::Expr(syn::Expr::Block(b)) if b.attrs.is_empty() && b.label.is_none() => strip_braces(&b.block),
            KVAttributeValue::Expr(e) => quote! { #e },
            KVAttributeValue::InvalidBraced(b) => crate::engine::nested_element_value(&b.body),
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

pub fn attr_value(key: &str, attr: &KeyedAttribute) -> TokenStream2 {
    if key.starts_with("on:") {
        if let Some(syn::Expr::Field(syn::ExprField { base, member, .. })) = attr_expr(attr) {
            return quote! { move |_| (#base).#member() };
        }
    }
    match key {
        "material" => {
            if let Some(list) = bracket_list(attr, &quote!(::riode::prelude::Material), &quote!(::riode::prelude::Stage)) {
                return list;
            }
            if let Some(syn::Expr::Closure(_)) = attr_expr(attr) {
                let inner = attr_value_tokens(attr);
                return quote! { ::riode::prelude::GeoFn(#inner) };
            }
            attr_value_tokens(attr)
        }
        "overlay" => {
            if let Some(list) = bracket_list(attr, &quote!(::riode::prelude::Overlay), &quote!(::riode::prelude::Stage)) {
                return list;
            }
            attr_value_tokens(attr)
        }
        "effect" => {
            if let Some(list) = bracket_list(attr, &quote!(::riode::prelude::Effect), &quote!(::riode::prelude::EffectStage)) {
                return list;
            }
            attr_value_tokens(attr)
        }
        _ => attr_value_tokens(attr),
    }
}

pub fn attr_expr(attr: &KeyedAttribute) -> Option<syn::Expr> {
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
