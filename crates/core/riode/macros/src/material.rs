// Single responsibility: interpret the `view!` material block at expansion
// time (D-M6) into the `material_ir` tree, then lower it to a WGSL `apply`
// module and emit the runtime construction tokens.
//
// This is the single lowering path (D-M6 section 2): `material={...}`
// blocks whose shape the interpreter recognizes never reach runtime Rust
// evaluation — they become (1) a `&'static str` WGSL source, (2) emitted
// uniform-filler code in arena order. Anything the interpreter does not
// recognize returns `None` and the attribute falls through to the normal
// expression semantics (raw `CustomEffect`, `EffectStack`, ...), so all
// existing authoring keeps working (D-M9).
//
// Interpretation rules (structure vs value — the load-bearing split):
// - Structure (decides generated WGSL, fixed at compile time): which
//   constructors appear, how they nest. Only these syntactic positions
//   are inspected: `let` bindings of constructor calls
//   (`Normal::flat()`, `Microfacet::ggx(n, a)`, `Sky::gain(g)`, ...),
//   `*` / `+` binary operators, `mix(a, b, w)` / `fresnel(n, ior)`
//   calls, and the terminal `.into_surface()`.
// - Value (rides uniforms at runtime): every numeric argument that is
//   not a float literal — identifiers (`GAIN`), field/method access,
//   `if` expressions on runtime state. Each takes one arena slot; the
//   emitted filler evaluates the original expression tokens in arena
//   order (fill == consume, no drift).
// - Structural branching is impossible: `if` in value position emits a
//   slot (the whole `if` becomes one uniform expression); `if` choosing
//   between two closures, or any `match`/`loop`/closure in the block,
//   is a loud compile error naming the rule (D-M6, reactivity rule).
// - `.with_*` post passes are rejected loudly (dropped D-M2/D-M10d).
//
// Phase 1 subset (mirrors `material_lower`): `Normal::flat`,
// `Microfacet::ggx`, `Sky::gain` / `Color` / `Intensity` modulators,
// `fresnel` / scalar weights, `+` / `*` / `mix` composition. Refraction,
// Beer, ThinFilm, and the extended closures are rejected loudly with
// their gate phase (5a/5b/5c/6) — never a silent mis-lowering.

use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};

use crate::material_ir::{
    Closure, Env, MaterialExpr, Modulator, Normal, UniformArena, Value, Weight,
};

/// One `let` binding the interpreter has classified: either a material
/// value (normal / weight / term — participates in the tree) or a plain
/// value binding (a runtime expression a later slot refers to — e.g.
/// `let g = hover;` — kept verbatim for filler emission).
#[derive(Clone)]
enum Binding {
    Normal(Normal),
    EnvGain(Value),
    Term(MaterialExpr),
    /// Whole-surface warp stage: the frequency/amplitude values (uniform
    /// slots, D-M10b) plus the compile-time pad literal (D-M8 pad fact).
    /// Kept separate from `Term(MaterialExpr)` so the pad cannot be
    /// dropped: `eval_term` reattaches it at the terminal.
    WarpTerm {
        frequency: Value,
        amplitude: Value,
        pad: f32,
    },
    Weight(Weight),
    /// Plain value binding: remembered tokens; uses allocate slots.
    /// (Slots allocate on first use, so emission order == fill order
    /// regardless of `let` order.)
    Value(TokenStream2),
}

/// Interpreter state for one `material={...}` block.
struct Interp {
    /// `Some(name)` when the block is the `|geo|` closure-param form:
    /// `geo.*` reads lower to per-pixel host fields, never slots.
    geo_param: Option<String>,
    /// True once any `geo.*` read was seen in this block.
    saw_geo: bool,
    /// True once any bare geometry constructor was seen in this block
    /// (flat/constant form). Mixing with `geo.*` is a loud error
    /// (one geometry source — D-M10a).
    saw_bare_geo: bool,
    arena: UniformArena,
    bindings: HashMap<String, Binding>,
    /// Value expressions in slot order: `filler[slot]` evaluates to f32.
    filler: Vec<TokenStream2>,
    /// `let` bindings emitted before the surface constructor (plain
    /// value bindings the filler refers to).
    preamble: Vec<TokenStream2>,
    /// Names already emitted into `preamble` (each `let` emits once).
    emitted: std::collections::HashSet<String>,
}

/// True when a path has exactly these segments (`Normal::flat`, `mix`).
fn path_is(path: &syn::Path, want: &[&str]) -> bool {
    path.segments.len() == want.len()
        && path
            .segments
            .iter()
            .zip(want.iter())
            .all(|(s, w)| s.ident == *w)
}

/// First path segment, as a string.
fn head_of(call: &syn::ExprCall) -> Option<String> {
    match call.func.as_ref() {
        syn::Expr::Path(p) => p
            .path
            .segments
            .iter()
            .next()
            .map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// Full dotted path of a call's function (`Microfacet::ggx`), else None.
fn func_path(call: &syn::ExprCall) -> Option<String> {
    match call.func.as_ref() {
        syn::Expr::Path(p) => Some(
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
        ),
        _ => None,
    }
}

impl Interp {
    fn new() -> Self {
        Self {
            geo_param: None,
            saw_geo: false,
            saw_bare_geo: false,
            arena: UniformArena::new(),
            bindings: HashMap::new(),
            filler: Vec::new(),
            preamble: Vec::new(),
            emitted: std::collections::HashSet::new(),
        }
    }

    /// A numeric position: literals and identifiers both allocate
    /// a uniform slot (D-M10b: every numeric parameter rides uniforms,
    /// never WGSL text). The emitted filler evaluates the remembered
    /// expression tokens in arena order (fill == consume, no drift).
    /// `Value::Lit` exists only for internal degenerate defaults, never
    /// for user-written numbers.
    fn numeric(&mut self, e: &syn::Expr) -> syn::Result<Value> {
        match e {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(_),
                ..
            })
            | syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(_),
                ..
            })
            | syn::Expr::Path(_) => {
                let slot = self.arena.alloc();
                // A slot referring to an earlier plain `let` binding
                // must see that binding's tokens: emit it into the
                // preamble once, before the constructor.
                if let syn::Expr::Path(p) = e {
                    if p.qself.is_none() && p.path.segments.len() == 1 {
                        let name = p.path.segments[0].ident.to_string();
                        if !self.emitted.contains(&name) {
                            if let Some(Binding::Value(tokens)) = self.bindings.get(&name) {
                                let ident =
                                    syn::Ident::new(&name, p.path.segments[0].ident.span());
                                let tokens = tokens.clone();
                                self.preamble.push(quote! { let #ident = #tokens; });
                                self.emitted.insert(name);
                            }
                        }
                    }
                }
                self.filler.push(e.to_token_stream());
                Ok(Value::Slot { slot })
            }
            other => {
                let slot = self.arena.alloc();
                self.filler.push(other.to_token_stream());
                Ok(Value::Slot { slot })
            }
        }
    }

    /// Peel the unit-tag wrappers `Radius::px(r)`, `Roughness::value(a)`,
    /// `Frequency::per_pixel(f)`, ... — the DSL's value wrappers. Only
    /// the known wrappers peel (inner expr classifies recursively);
    /// unknown calls are NOT silently values — a loud error so a typo
    /// cannot become a slot.
    /// `Amplitude::px(a)` peels here too — the warp node reads both
    /// wrappers directly (Phase 4.5), so `Warp::sine` arg shapes stay
    /// visible to the `WarpExpr` classifier below.
    fn peel_wrapper(&mut self, e: &syn::Expr) -> syn::Result<Option<Value>> {
        let syn::Expr::Call(call) = e else {
            return Ok(None);
        };
        let Some(fp) = func_path(call) else {
            return Ok(None);
        };
        let known = matches!(
            fp.as_str(),
            "Radius::px"
                | "Offset::px"
                | "Spacing::px"
                | "Width::px"
                | "Curvature::Convex"
                | "Curvature::Concave"
                | "Frequency::per_pixel"
                | "Amplitude::px"
                | "Scale::factor"
                | "Density::factor"
                | "Thickness::nm"
                | "Ior::film"
                | "Ior::value"
                | "Roughness::value"
                | "Roughness::x"
                | "Roughness::y"
        );
        if !known {
            return Ok(None);
        }
        if call.args.len() != 1 {
            return Err(syn::Error::new_spanned(
                &call.func,
                format!("material DSL: `{fp}` takes exactly one argument"),
            ));
        }
        Ok(Some(self.numeric(&call.args[0])?))
    }

    /// `Spread::px(s)` — dispersion width in px (D-M10c preset param).
    fn peel_spread(&mut self, e: &syn::Expr) -> syn::Result<Option<Value>> {
        let syn::Expr::Call(call) = e else {
            return Ok(None);
        };
        let Some(fp) = func_path(call) else {
            return Ok(None);
        };
        if fp != "Spread::px" {
            return Ok(None);
        }
        if call.args.len() != 1 {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL: `Spread::px(s)` takes exactly one argument",
            ));
        }
        Ok(Some(self.numeric(&call.args[0])?))
    }

    /// `Thickness::px(d)` — slab depth in px (D-M10c preset param).
    fn peel_thickness(&mut self, e: &syn::Expr) -> syn::Result<Option<Value>> {
        let syn::Expr::Call(call) = e else {
            return Ok(None);
        };
        let Some(fp) = func_path(call) else {
            return Ok(None);
        };
        if fp != "Thickness::px" {
            return Ok(None);
        }
        if call.args.len() != 1 {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL: `Thickness::px(d)` takes exactly one argument",
            ));
        }
        Ok(Some(self.numeric(&call.args[0])?))
    }

    /// `Tint(color)` — slab tint: a `Color::rgb/call/const` lowers to
    /// three slotted channels (D-M10b).
    fn peel_tint(&mut self, e: &syn::Expr) -> syn::Result<(Value, Value, Value)> {
        let syn::Expr::Call(call) = e else {
            return Err(syn::Error::new_spanned(
                e,
                "material DSL (Phase 4): `Slab::thick` wants `Tint(color)` — \
                 `Tint(Color::rgb(r, g, b))` or a named color",
            ));
        };
        let Some(fp) = func_path(call) else {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 4): `Slab::thick` wants `Tint(color)`",
            ));
        };
        if fp != "Tint" {
            return Err(syn::Error::new_spanned(
                &call.func,
                format!(
                    "material DSL (Phase 4): `Slab::thick` wants `Tint(color)`, not `{fp}`"
                ),
            ));
        }
        if call.args.len() != 1 {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL: `Tint(color)` takes exactly one argument",
            ));
        }
        let arg = &call.args[0];
        // `Color::rgb(r, g, b)` call form.
        if let syn::Expr::Call(c) = arg {
            if let syn::Expr::Path(p) = c.func.as_ref() {
                if path_is(&p.path, &["Color", "rgb"]) {
                    if c.args.len() != 3 {
                        return Err(syn::Error::new_spanned(
                            &c.func,
                            "material DSL: `Color::rgb` takes exactly 3 arguments",
                        ));
                    }
                    return Ok((
                        self.numeric(&c.args[0])?,
                        self.numeric(&c.args[1])?,
                        self.numeric(&c.args[2])?,
                    ));
                }
                if let Some((r, g, b)) = color_name(&p.path) {
                    if !c.args.is_empty() {
                        return Err(syn::Error::new_spanned(
                            &c.func,
                            "material DSL: `Color::named()` takes no arguments",
                        ));
                    }
                    return Ok((self.lit_slot(r)?, self.lit_slot(g)?, self.lit_slot(b)?));
                }
            }
        }
        // `Color::RED`-style const path or a runtime color expression:
        // resolve consts to slotted channels, anything else errors
        // loudly (tint is a value triple, not an arbitrary term).
        if let syn::Expr::Path(p) = arg {
            if p.qself.is_none() {
                if let Some((r, g, b)) = const_color_name(&p.path) {
                    return Ok((self.lit_slot(r)?, self.lit_slot(g)?, self.lit_slot(b)?));
                }
            }
        }
        Err(syn::Error::new_spanned(
            arg,
            "material DSL (Phase 4): `Tint` wants a color — \
             `Color::rgb(r, g, b)` or a named color",
        ))
    }

    /// `Absorb(k)` — Beer density factor (D-M10c preset param).
    fn peel_absorb(&mut self, e: &syn::Expr) -> syn::Result<Option<Value>> {
        let syn::Expr::Call(call) = e else {
            return Ok(None);
        };
        let Some(fp) = func_path(call) else {
            return Ok(None);
        };
        if fp != "Absorb" {
            return Ok(None);
        }
        if call.args.len() != 1 {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL: `Absorb(k)` takes exactly one argument",
            ));
        }
        Ok(Some(self.numeric(&call.args[0])?))
    }

    /// One slotted literal channel (preset defaults / named colors —
    /// D-M10b: every numeric rides a slot, never WGSL text).
    fn lit_slot(&mut self, v: f32) -> syn::Result<Value> {
        let slot = self.arena.alloc();
        self.filler.push(syn::parse_str::<syn::Expr>(&format!("{v:?}")).unwrap().to_token_stream());
        Ok(Value::Slot { slot })
    }

    /// The preset base IOR default (1.5 crown glass): a slotted
    /// literal, so the source never bakes it (D-M10b).
    fn bare_ior(&mut self) -> Value {
        let slot = self.arena.alloc();
        self.filler.push(
            syn::parse_str::<syn::Expr>("1.5").unwrap().to_token_stream(),
        );
        Value::Slot { slot }
    }

    /// Classify an expression into a material binding value.
    fn eval(&mut self, e: &syn::Expr) -> syn::Result<Binding> {
        // Benign parens (`(a * b)`, `((x))`) unwrap — the DSL is
        // parenthesized per Rust precedence (§1.3: `*` binds tighter
        // than `+`; examples parenthesize explicitly).
        if let syn::Expr::Paren(p) = e {
            return self.eval(&p.expr);
        }
        if let syn::Expr::Group(g) = e {
            return self.eval(&g.expr);
        }
        // `geo.normal` / `geo.thickness` / ... field reads (D-M10a):
        // only legal in the `|geo|` closure-param form; lower to
        // per-pixel host-field bindings (never uniform slots).
        if let syn::Expr::Field(f) = e {
            if let syn::Expr::Path(base) = f.base.as_ref() {
                if base.qself.is_none() && base.path.segments.len() == 1 {
                    let name = base.path.segments[0].ident.to_string();
                    if Some(name.clone()) == self.geo_param {
                        return self.eval_geo_field(&f.member);
                    }
                }
            }
        }
        // `Color::RED` / `Color::WHITE` const paths (syn `Path`, not
        // `Call`): the const-color term, same as `Color::red()`.
        if let syn::Expr::Path(p) = e {
            if p.qself.is_none() {
                if let Some((r, g, b)) = const_color_name(&p.path) {
                    return Ok(Binding::Term(self.const_color_term(r, g, b)));
                }
                if p.path.segments.len() == 1 {
                    let name = p.path.segments[0].ident.to_string();
                    match self.bindings.get(&name) {
                        Some(b) => return Ok(b.clone()),
                        None => {
                            return Err(syn::Error::new_spanned(
                                &p.path,
                                format!(
                                    "material DSL: unknown binding `{name}` — bind a material value \
                                     with `let` first (structure is fixed at compile time)"
                                ),
                            ));
                        }
                    }
                }
            }
        }
        match e {
            syn::Expr::Call(call) => self.eval_call(call),
            syn::Expr::Binary(b) => self.eval_binary(b),
            syn::Expr::MethodCall(m) => self.eval_method(m),
            syn::Expr::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => {
                let name = p.path.segments[0].ident.to_string();
                match self.bindings.get(&name) {
                    Some(b) => Ok(b.clone()),
                    None => Err(syn::Error::new_spanned(
                        &p.path,
                        format!(
                            "material DSL: unknown binding `{name}` — bind a material value \
                             with `let` first (structure is fixed at compile time)"
                        ),
                    )),
                }
            }
            other => Err(syn::Error::new_spanned(
                other,
                format!(
                    "material DSL (Phase 1): cannot interpret `{}` here — material blocks hold \
                     material constructors, `*` / `+`, `mix` / `fresnel`, and `.into_surface()`; \
                     runtime `if` is value-level only (inside a numeric argument)",
                    other.to_token_stream()
                ),
            )),
        }
    }

    /// Mark a bare (context-free) geometry constructor. Mixing with
    /// `geo.*` reads in one block is a loud error (one source — D-M10a).
    fn mark_bare_geo(&mut self, span: &dyn quote::ToTokens) -> syn::Result<()> {
        if self.saw_geo {
            return Err(syn::Error::new_spanned(
                span,
                "material DSL (Phase 2): one geometry source per block — bare geometry \
                 constructors (`Normal::flat()`) cannot mix with `geo.*` reads; pick the \
                 bare `material={ ... }` form (flat/constant) or `material={ |geo| ... }`",
            ));
        }
        self.saw_bare_geo = true;
        Ok(())
    }

    /// Lower one `geo.*` field read (D-M10a). Phase 2 proves plumbing
    /// with geometry-driven weights/tints only: `.normal` binds the
    /// flat normal (geometry tracking verified by the host-field read,
    /// not by optical content — `Refract` still gates Phase 5a).
    /// Unknown fields are a loud error.
    fn eval_geo_field(&mut self, member: &syn::Member) -> syn::Result<Binding> {
        let field = match member {
            syn::Member::Named(id) => id.to_string(),
            syn::Member::Unnamed(ix) => {
                return Err(syn::Error::new(
                    ix.span,
                    "material DSL (Phase 2): `geo` fields are named \
                     (`geo.normal`, `geo.thickness`, `geo.edge`) — no tuple access",
                ));
            }
        };
        if self.geo_param.is_none() {
            return Err(syn::Error::new_spanned(
                member,
                "material DSL (Phase 2): `geo.*` reads need the closure-param form \
                 `material={ |geo| ... }` — the bare block means flat/constant geometry",
            ));
        }
        if self.saw_bare_geo {
            return Err(syn::Error::new_spanned(
                member,
                "material DSL (Phase 2): one geometry source per block — `geo.*` reads \
                 cannot mix with bare geometry constructors (`Normal::flat()`); pick one",
            ));
        }
        self.saw_geo = true;
        match field.as_str() {
            // Phase 2 plumbing: `geo.normal` resolves to the flat
            // normal; geometry-dependent closures (`Refract`,
            // `fresnel(geo.normal, ...)`) still gate Phase 5a/5b and
            // reject loudly there — Phase 2 proves the HOST-FIELD
            // path, not the optics.
            "normal" => Ok(Binding::Normal(Normal::GeoNormal)),
            // `geo.thickness` / `geo.edge` / `geo.position` /
            // `geo.curvature` lower to host-field-driven SCALAR weights
            // (uniform-slot expressions the lowerer renders as the
            // corresponding `ctx` reads — Phase 5a wires the real
            // fields; Phase 2 proves variation tracks geometry via
            // the edge-driven `mix` demo weight).
            "thickness" | "edge" | "position" | "curvature" => {
                Ok(Binding::Weight(Weight::GeoField(field)))
            }
            _ => Err(syn::Error::new_spanned(
                member,
                format!(
                    "material DSL (Phase 2): unknown `geo` field `.{field}` — known: \
                     `.normal`, `.position`, `.curvature`, `.thickness`, `.edge`"
                ),
            )),
        }
    }

    /// Constructor calls: `Normal::flat()`, `Microfacet::ggx(n, a)`,
    /// `Sky::gain(g)`, `Color::rgb(r,g,b)`, `fresnel(n, ior)`, `mix(...)`.
    fn eval_call(&mut self, call: &syn::ExprCall) -> syn::Result<Binding> {
        let syn::Expr::Path(p) = call.func.as_ref() else {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL: only plain constructor paths are material constructors",
            ));
        };
        if path_is(&p.path, &["Normal", "flat"]) {
            expect_no_args(call, "Normal::flat()")?;
            self.mark_bare_geo(call)?;
            return Ok(Binding::Normal(Normal::Flat));
        }
        if path_is(&p.path, &["Sky", "gain"]) {
            let args = expect_args(call, "Sky::gain(g)", 1)?;
            return Ok(Binding::EnvGain(self.numeric(args[0])?));
        }
        if path_is(&p.path, &["Microfacet", "ggx"]) {
            let args = expect_args(call, "Microfacet::ggx(n, roughness)", 2)?;
            let normal = self.eval_normal(args[0])?;
            let roughness = self.value_of(args[1])?;
            return Ok(Binding::Term(MaterialExpr::Closure(Closure::Ggx {
                normal,
                roughness,
            })));
        }
        if path_is(&p.path, &["Color", "rgb"]) {
            let args = expect_args(call, "Color::rgb(r, g, b)", 3)?;
            return Ok(Binding::Term(self.color_term(args[0], args[1], args[2])?));
        }
        if let Some((r, g, b)) = color_name(&p.path) {
            expect_no_args(call, "Color::named()")?;
            // D-M10b: named colors slot per channel (same as the const
            // path and `Color::rgb` forms) — no literal reaches WGSL.
            return Ok(Binding::Term(self.named_color_term(r, g, b)));
        }
        if path_is(&p.path, &["Intensity", "factor"]) || path_is(&p.path, &["Intensity", "lumens"])
        {
            let args = expect_args(call, "Intensity::factor/lumens(x)", 1)?;
            let lumens = self.numeric(args[0])?;
            // Bare intensity scales the unit ambient; `*` attaches it
            // to a real term (same degenerate-inner replacement).
            return Ok(Binding::Term(MaterialExpr::Modulated(
                Modulator::Intensity {
                    lumens: lumens.clone(),
                },
                Box::new(MaterialExpr::Closure(Closure::Ggx {
                    normal: Normal::Flat,
                    roughness: Value::Lit(1.0),
                })),
            )));
        }
        if path_is(&p.path, &["fresnel"]) {
            let args = expect_args(call, "fresnel(n, ior)", 2)?;
            let normal = self.eval_normal(args[0])?;
            let ior = self.value_of(args[1])?;
            return Ok(Binding::Weight(Weight::Fresnel { normal, ior }));
        }
        if path_is(&p.path, &["mix"]) {
            let args = expect_args(call, "mix(a, b, weight)", 3)?;
            let a = self.eval_term(args[0])?;
            let b = self.eval_term(args[1])?;
            let w = self.eval_weight(args[2])?;
            return Ok(Binding::Term(MaterialExpr::Mix(
                Box::new(a),
                Box::new(b),
                w,
            )));
        }
        // Phase 4.5 warp node (literal fast path): `Warp::sine(
        // Frequency::per_pixel(<lit>), Amplitude::px(<lit>))` bakes both
        // params as `Value::Lit` — the warp module inlines them into WGSL
        // (`warp_body_wgsl`), no uniform slots, no filler. The `pad_literal`
        // fact doubles as the baked amplitude (one literal, two uses —
        // value and reach can never drift). Non-literals are a loud error:
        // a runtime amplitude would move displaced reads outside the
        // snapshot region the compositor already captured (D-M8), and a
        // whole-stage closure cannot vary per frame inside an `Option<_>`
        // prop without the host's `Prop::Dynamic` form (which the component
        // boundary does not have).
        if path_is(&p.path, &["Warp", "sine"]) {
            let args = expect_args(
                call,
                "Warp::sine(Frequency::per_pixel(f), Amplitude::px(a))",
                2,
            )?;
            let frequency = Self::warp_lit_param(args[0])?;
            let amplitude = Self::warp_lit_param(args[1])?;
            let pad = Self::pad_literal(args[1])?;
            return Ok(Binding::WarpTerm {
                frequency,
                amplitude,
                pad,
            });
        }
        if path_is(&p.path, &["Refract", "new"]) {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 1): `Refract` lowers in Phase 5a \
                 (displaced backdrop reads + frost blur) — use `Microfacet::ggx` / `Sky` / `Color` until then",
            ));
        }
        // Phase 4 presets (D-M10c): `Dispersion::prism` / `Slab::thick`
        // parse now (uniform-bound params), reject at lowering until
        // their Phase 5 optics land (G1* unaffected — spec only).
        if path_is(&p.path, &["Dispersion", "prism"]) {
            let args = expect_args(call, "Dispersion::prism(spread)", 1)?;
            let spread = match self.peel_spread(&args[0])? {
                Some(v) => v,
                None => self.value_of(&args[0])?,
            };
            return Ok(Binding::Term(MaterialExpr::Closure(Closure::Dispersion {
                normal: Normal::Flat,
                base_ior: self.bare_ior(),
                spread_px: spread,
            })));
        }
        if path_is(&p.path, &["Slab", "thick"]) {
            let args = expect_args(call, "Slab::thick(thickness, tint, absorb)", 3)?;
            let thickness = match self.peel_thickness(&args[0])? {
                Some(v) => v,
                None => self.value_of(&args[0])?,
            };
            let (tr, tg, tb) = self.peel_tint(&args[1])?;
            let absorb = match self.peel_absorb(&args[2])? {
                Some(v) => v,
                None => self.value_of(&args[2])?,
            };
            return Ok(Binding::Term(MaterialExpr::Closure(Closure::Slab {
                normal: Normal::Flat,
                thickness_px: thickness,
                tint_r: tr,
                tint_g: tg,
                tint_b: tb,
                absorb,
            })));
        }
        if path_is(&p.path, &["tag_bloom"]) || path_is(&p.path, &["tag_hdr"]) {
            let what = if path_is(&p.path, &["tag_bloom"]) {
                "tag_bloom"
            } else {
                "tag_hdr"
            };
            return Err(syn::Error::new_spanned(
                &call.func,
                format!(
                    "material DSL (Phase 4): `{what}` is a frame-wide global post tag \
                     (D-M10d) — declare it outside any `material={{...}}` block \
                     (root/frame scope, settled when Step 5 lands the HDR post host); \
                     parsed-but-no-op until then"
                ),
            ));
        }
        if path_is(&p.path, &["Absorption", "beer"]) {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 1): `Absorption::beer` lowers in Phase 5c \
                 (needs `path_length` from displaced reads)",
            ));
        }
        if path_is(&p.path, &["ThinFilm", "interference"]) {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 1): `ThinFilm::interference` lowers in Phase 6 \
                 (extended closures)",
            ));
        }
        if let Some(h) = head_of(call) {
            if ["Microfacet", "Diffuse", "Sheen", "Conductor", "Emission", "Lens", "Bump"]
                .contains(&h.as_str())
            {
                return Err(syn::Error::new_spanned(
                    &call.func,
                    format!(
                        "material DSL (Phase 1): `{h}` lowers after Phase 1 (5a normals/Refract, \
                         3 extended closures) — core set is flat + ggx + Color/Intensity/Sky + fresnel"
                    ),
                ));
            }
        }
        Err(syn::Error::new_spanned(
            &call.func,
            "material DSL: unknown constructor — Phase 1 knows `Normal::flat`, \
             `Microfacet::ggx`, `Sky::gain`, `Color::rgb/white/red/green/blue`, \
             `Intensity::factor/lumens`, `fresnel`, `mix`",
        ))
    }

    fn color_term(
        &mut self,
        r: &syn::Expr,
        g: &syn::Expr,
        b: &syn::Expr,
    ) -> syn::Result<MaterialExpr> {
        Ok(MaterialExpr::Modulated(
            Modulator::Color {
                r: self.numeric(r)?,
                g: self.numeric(g)?,
                b: self.numeric(b)?,
            },
            Box::new(MaterialExpr::Closure(Closure::Ggx {
                normal: Normal::Flat,
                roughness: Value::Lit(1.0),
            })),
        ))
    }

    fn named_color_term(&mut self, r: f32, g: f32, b: f32) -> MaterialExpr {
        let mut slot_for = |v: f32| {
            let slot = self.arena.alloc();
            self.filler.push(syn::parse_str::<syn::Expr>(&format!("{v:?}")).unwrap().to_token_stream());
            Value::Slot { slot }
        };
        MaterialExpr::Modulated(
            Modulator::Color {
                r: slot_for(r),
                g: slot_for(g),
                b: slot_for(b),
            },
            // Bare `Color` in term position tints the unit ambient (a
            // degenerate lobe — the `1.0` roughness is structural
            // identity, never a user numeric); `*` with a real closure
            // replaces the inner term (factors attach inward).
            Box::new(MaterialExpr::Closure(Closure::Ggx {
                normal: Normal::Flat,
                roughness: Value::Lit(1.0),
            })),
        )
    }

    /// `Color::RED`-style const path in term position: same slotted
    /// shape as the call form (D-M10b — no numeric reaches WGSL text),
    /// so `*` attaches inward identically.
    fn const_color_term(&mut self, r: f32, g: f32, b: f32) -> MaterialExpr {
        self.named_color_term(r, g, b)
    }

    /// `a * b` (factors attach: Color/Intensity/Sky onto a term) and
    /// `a + b` (radiance sum) / `n1 + n2` (normal sum — Phase 5a shape,
    /// rejected by the lowerer with its phase).
    fn eval_binary(&mut self, b: &syn::ExprBinary) -> syn::Result<Binding> {
        match &b.op {
            syn::BinOp::Mul(_) => {
                let l = self.eval(&b.left)?;
                let r = self.eval(&b.right)?;
                Ok(Binding::Term(self.mul(l, r)?))
            }
            syn::BinOp::Add(_) => {
                let l = self.eval(&b.left)?;
                let r = self.eval(&b.right)?;
                match (l, r) {
                    (Binding::Term(a), Binding::Term(b)) => {
                        Ok(Binding::Term(MaterialExpr::Sum(Box::new(a), Box::new(b))))
                    }
                    (Binding::Normal(a), Binding::Normal(b)) => {
                        Ok(Binding::Normal(Normal::Sum(Box::new(a), Box::new(b))))
                    }
                    _ => Err(syn::Error::new_spanned(
                        &b.op,
                        "material DSL: `+` sums two material terms (radiance sum) \
                         or two normals — weights and env gains compose with `*` / `mix`",
                    )),
                }
            }
            _ => Err(syn::Error::new_spanned(
                &b.op,
                "material DSL: only `*` (modulate) and `+` (radiance sum) compose materials",
            )),
        }
    }

    /// Attach factors: `Color * term` (replace the degenerate inner),
    /// `term * Sky::gain` (light it), `term * Intensity` (scale it),
    /// `term * scalar-expr` (uniform scale — value-level hover).
    fn mul(&mut self, l: Binding, r: Binding) -> syn::Result<MaterialExpr> {
        use Binding::{EnvGain, Term, Value as Val};
        match (l, r) {
            (EnvGain(gain), Term(t)) => Ok(apply_env(t, gain)),
            (Term(t), EnvGain(gain)) => Ok(apply_env(t, gain)),
            (Term(t), Val(tokens)) | (Val(tokens), Term(t)) => {
                let slot = self.arena.alloc();
                self.filler.push(tokens);
                Ok(MaterialExpr::Modulated(
                    Modulator::Intensity {
                        lumens: Value::Slot { slot },
                    },
                    Box::new(t),
                ))
            }
            (Term(a), Term(b)) => Ok(attach_color(a, b)),
            _ => Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "material DSL: `*` composes colors / `Sky::gain` / intensities \
                 with material terms — weights use `mix`, terms sum with `+`",
            )),
        }
    }

    /// `.into_surface()` (terminal) and `.frost(r)` (Phase 5a shape,
    /// rejected loudly). `.with_*` per-Group post passes rejected as
    /// DROPPED (D-M10d — use frame-wide `tag_bloom`/`tag_hdr`).
    fn eval_method(&mut self, m: &syn::ExprMethodCall) -> syn::Result<Binding> {
        let name = m.method.to_string();
        match name.as_str() {
            "into_surface" => {
                if !m.args.is_empty() || m.turbofish.is_some() {
                    return Err(syn::Error::new_spanned(
                        &m.method,
                        "material DSL: `.into_surface()` takes no arguments",
                    ));
                }
                let term = self.eval_term(&m.receiver)?;
                Ok(Binding::Term(term))
            }
            "frost" => Err(syn::Error::new_spanned(
                &m.method,
                "material DSL (Phase 1): `.frost(r)` chains blur onto `Refract`, \
                 which lowers in Phase 5a — omit it until then",
            )),
            _ if name.starts_with("with_") => Err(syn::Error::new_spanned(
                &m.method,
                format!(
                    "material DSL (D-M10d): `.{name}(...)` is a per-Group post pass — DROPPED \
                     (`[0]`), not deferred: post phenomena cannot live per-Group (N overlapping \
                     BloomGroups must not allocate N pyramids). Declare the frame-wide global \
                     tag instead (consumed once by the HDR post host when Step 5 lands): \
                     `tag_bloom(strength, Radius)` / `tag_hdr(exposure)` outside any \
                     `material={{...}}` block. v1 surfaces end at `.into_surface()`"
                ),
            )),
            _ => Err(syn::Error::new_spanned(
                &m.method,
                format!(
                    "material DSL: unknown method `.{name}` — v1 knows `.into_surface()` \
                     (terminal) and `.frost` (Phase 5a, on `Refract`)"
                ),
            )),
        }
    }

    fn eval_normal(&mut self, e: &syn::Expr) -> syn::Result<Normal> {
        match self.eval(e)? {
            Binding::Normal(n) => Ok(n),
            _ => Err(syn::Error::new_spanned(
                e,
                "material DSL: expected a normal field here (`Normal::flat()` in Phase 1)",
            )),
        }
    }

    fn eval_term(&mut self, e: &syn::Expr) -> syn::Result<MaterialExpr> {
        match self.eval(e)? {
            Binding::Term(t) => Ok(t),
            Binding::WarpTerm {
                frequency,
                amplitude,
                ..
            } => Ok(MaterialExpr::Closure(Closure::Warp {
                frequency,
                amplitude,
            })),
            _ => Err(syn::Error::new_spanned(
                e,
                "material DSL: expected a material term here (a closure, `Color * ...`, `a + b`, `mix(...)`)",
            )),
        }
    }

    /// Weight position: `fresnel(n, ior)` (or a `let`-bound weight), else
    /// any scalar expression → one uniform slot (literals inline).
    fn eval_weight(&mut self, e: &syn::Expr) -> syn::Result<Weight> {
        // `geo.*` scalar fields in weight position (`mix(a, b, geo.edge)`):
        // route through the classifier so a `GeoField` binding returns
        // its host-field weight. Anything else falls to the numeric slot
        // below — never silently (a `geo.normal` here errors loudly in
        // `eval`, a normal is not a weight).
        if matches!(e, syn::Expr::Field(_)) {
            match self.eval(e)? {
                Binding::Weight(w) => return Ok(w),
                _ => {
                    return Err(syn::Error::new_spanned(
                        e,
                        "material DSL: weight position wants `fresnel(...)`, a scalar \
                         expression, or a `geo.*` scalar field (`geo.edge`, `geo.thickness`, \
                         `geo.position`, `geo.curvature`) — normals are not weights",
                    ));
                }
            }
        }
        // `fresnel(...)` call in weight position.
        if let syn::Expr::Call(call) = e {
            if let syn::Expr::Path(p) = call.func.as_ref() {
                if path_is(&p.path, &["fresnel"]) {
                    let args = expect_args(call, "fresnel(n, ior)", 2)?;
                    return Ok(Weight::Fresnel {
                        normal: self.eval_normal(args[0])?,
                        ior: self.value_of(args[1])?,
                    });
                }
                if path_is(&p.path, &["mix"]) {
                    return Err(syn::Error::new_spanned(
                        e,
                        "material DSL: `mix` in weight position is not supported — \
                         weights are `fresnel(...)` or scalar expressions",
                    ));
                }
            }
        }
        // `let w = fresnel(...);` then `mix(a, b, w)`.
        if let syn::Expr::Path(p) = e {
            if p.qself.is_none() && p.path.segments.len() == 1 {
                let name = p.path.segments[0].ident.to_string();
                if let Some(Binding::Weight(w)) = self.bindings.get(&name) {
                    return Ok(w.clone());
                }
            }
        }
        Ok(Weight::Scalar(self.numeric(e)?))
    }

    /// A numeric-or-wrapper-or-plain-value position: peel known unit
    /// wrappers first, then literals/slots. A bare path bound with `let`
    /// to a plain value (`let g = ...;`) resolves through the remembered
    /// tokens (emitted into the preamble once).
    fn value_of(&mut self, e: &syn::Expr) -> syn::Result<Value> {
        if let Some(v) = self.peel_wrapper(e)? {
            return Ok(v);
        }
        if let syn::Expr::Path(p) = e {
            if p.qself.is_none() && p.path.segments.len() == 1 {
                let name = p.path.segments[0].ident.to_string();
                if let Some(Binding::Value(tokens)) = self.bindings.get(&name) {
                    let tokens = tokens.clone();
                    if !self.emitted.contains(&name) {
                        let ident = syn::Ident::new(&name, p.path.segments[0].ident.span());
                        self.preamble.push(quote! { let #ident = #tokens; });
                        self.emitted.insert(name);
                    }
                    let slot = self.arena.alloc();
                    let ident = &p.path.segments[0].ident;
                    self.filler.push(quote! { (#ident as f32) });
                    return Ok(Value::Slot { slot });
                }
            }
        }
        self.numeric(e)
    }

    /// The D-M8 pad fact for a warp amplitude position: the compile-time
    /// literal inside `Amplitude::px(<lit>)`, or a bare float/int literal
    /// (the unwrapped form). Anything else (runtime expr, signal read,
    /// `let`-bound path) is a loud error — a runtime pad would size the
    /// snapshot region AFTER the compositor already captured it.
    ///
    /// Literal warp param (Phase 4.5 fast path): peel the unit wrapper
    /// (`Frequency::per_pixel(<lit>)` / `Amplitude::px(<lit>)`) or a bare
    /// float/int literal into a `Value::Lit` the WGSL bakes inline. This
    /// is the deliberate exception to D-M10b (uniform-bound numerics):
    /// the warp displacement IS the surface shape for this demo row, and
    /// the component boundary (`Option<EffectStack>`, `impl Into<_>`) has
    /// no `Prop::Dynamic` form — a slotted warp would emit `move || …`
    /// and fail the setter bound. Non-literals are a loud error here
    /// (same D-M8 rule as `pad_literal`): re-express the variation with
    /// `.then(...)` chains or a signal-driven parent.
    fn warp_lit_param(e: &syn::Expr) -> syn::Result<Value> {
        // `Frequency::per_pixel(<lit>)` / `Amplitude::px(<lit>)` wrapper.
        if let syn::Expr::Call(call) = e {
            if let Some(fp) = func_path(call) {
                if fp == "Amplitude::px" || fp == "Frequency::per_pixel" {
                    if call.args.len() != 1 {
                        return Err(syn::Error::new_spanned(
                            &call.func,
                            format!("material DSL: `{fp}` takes exactly one argument"),
                        ));
                    }
                    return Self::warp_lit_param(&call.args[0]);
                }
            }
        }
        // Bare literal.
        if let syn::Expr::Lit(syn::ExprLit { lit, .. }) = e {
            match lit {
                syn::Lit::Float(f) => {
                    let v: f32 = f.base10_parse().map_err(|_| {
                        syn::Error::new_spanned(e, "material DSL: bad float literal")
                    })?;
                    return Ok(Value::Lit(v));
                }
                syn::Lit::Int(i) => {
                    let v: f32 = i.base10_parse().map_err(|_| {
                        syn::Error::new_spanned(e, "material DSL: bad int literal")
                    })?;
                    return Ok(Value::Lit(v));
                }
                _ => {}
            }
        }
        Err(syn::Error::new_spanned(
            e,
            "material DSL (Phase 4.5): warp params must be literals \
             (`Frequency::per_pixel(<lit>)`, `Amplitude::px(<lit>)`) — the warp \
             module bakes them into WGSL and sizes the snapshot from the same \
             literal (D-M8), so a runtime expression here would move displaced \
             reads outside the captured region; re-express the variation with \
             `.then(...)` chains or a signal-driven parent",
        ))
    }
    fn pad_literal(e: &syn::Expr) -> syn::Result<f32> {
        // `Amplitude::px(<lit>)` wrapper form.
        if let syn::Expr::Call(call) = e {
            if let Some(fp) = func_path(call) {
                if fp == "Amplitude::px" {
                    if call.args.len() != 1 {
                        return Err(syn::Error::new_spanned(
                            &call.func,
                            "material DSL: `Amplitude::px(a)` takes exactly one argument",
                        ));
                    }
                    return Self::pad_literal(&call.args[0]);
                }
            }
        }
        // Bare literal form.
        if let syn::Expr::Lit(syn::ExprLit { lit, .. }) = e {
            match lit {
                syn::Lit::Float(f) => {
                    return f.base10_parse::<f32>().map_err(|_| {
                        syn::Error::new_spanned(e, "material DSL: bad float literal")
                    });
                }
                syn::Lit::Int(i) => {
                    return i.base10_parse::<f32>().map_err(|_| {
                        syn::Error::new_spanned(e, "material DSL: bad int literal")
                    });
                }
                _ => {}
            }
        }
        Err(syn::Error::new_spanned(
            e,
            "material DSL (Phase 4.5): warp amplitude must be an `Amplitude::px(<literal>)` \
             so `Effect::pad` can size the snapshot region at compile time (D-M8); \
             a runtime expression here would move displaced reads outside the captured region",
        ))
    }
}

/// `term * Sky::gain(g)` / `Sky::gain(g) * term`: light the term's
/// closures with gain `g`. Distributes over `Sum` (each lobe keeps its
/// own env — factors commute); wraps bare closures in `Lit`.
fn apply_env(term: MaterialExpr, gain: Value) -> MaterialExpr {
    match term {
        MaterialExpr::Closure(c) => MaterialExpr::Lit(c, Env { gain }),
        MaterialExpr::Lit(c, _) => MaterialExpr::Lit(c, Env { gain }),
        MaterialExpr::Sum(a, b) => MaterialExpr::Sum(
            Box::new(apply_env(*a, gain.clone())),
            Box::new(apply_env(*b, gain)),
        ),
        MaterialExpr::Mix(a, b, w) => MaterialExpr::Mix(
            Box::new(apply_env(*a, gain.clone())),
            Box::new(apply_env(*b, gain)),
            w,
        ),
        MaterialExpr::Modulated(m, inner) => MaterialExpr::Modulated(m, Box::new(apply_env(*inner, gain))),
    }
}

/// `Color * term`: if one side carries the degenerate ambient lobe
/// (bare `Color` / bare `Intensity` in term position), its modulator
/// re-wraps the real term; otherwise the two nest (factors commute,
/// grouping unobservable).
fn attach_color(color_side: MaterialExpr, term_side: MaterialExpr) -> MaterialExpr {
    match (color_side, term_side) {
        (MaterialExpr::Modulated(m, inner), t) if is_degenerate_ambient(&inner) => {
            MaterialExpr::Modulated(m, Box::new(t))
        }
        (t, MaterialExpr::Modulated(m, inner)) if is_degenerate_ambient(&inner) => {
            MaterialExpr::Modulated(m, Box::new(t))
        }
        (a, b) => MaterialExpr::Sum(Box::new(a), Box::new(b)),
    }
}

/// The placeholder lobe a bare `Color` / bare `Intensity` carries:
/// flat-normal GGX at roughness 1.0 — replaced by `attach_color` when
/// the modulator actually meets a term via `*`.
fn is_degenerate_ambient(e: &MaterialExpr) -> bool {
    matches!(
        e,
        MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Lit(r),
        }) if *r == 1.0
    )
}

/// `Color::white/red/green/blue` — the albedo, else `None`.
fn color_name(path: &syn::Path) -> Option<(f32, f32, f32)> {
    if path.segments.len() != 2 {
        return None;
    }
    let mut it = path.segments.iter();
    let (a, b) = (it.next().unwrap(), it.next().unwrap());
    if a.ident != "Color" {
        return None;
    }
    if b.ident == "white" || b.ident == "WHITE" {
        Some((1.0, 1.0, 1.0))
    } else if b.ident == "red" || b.ident == "RED" {
        Some((1.0, 0.0, 0.0))
    } else if b.ident == "green" || b.ident == "GREEN" {
        Some((0.0, 1.0, 0.0))
    } else if b.ident == "blue" || b.ident == "BLUE" {
        Some((0.0, 0.0, 1.0))
    } else {
        None
    }
}

/// `Color::RED`-style associated-const path — the same four colors as
/// a const path (syn `Path`, no call args).
fn const_color_name(path: &syn::Path) -> Option<(f32, f32, f32)> {
    color_name(path)
}

fn expect_no_args(call: &syn::ExprCall, what: &str) -> syn::Result<()> {
    if !call.args.is_empty() {
        return Err(syn::Error::new_spanned(
            &call.func,
            format!("material DSL: `{what}` takes no arguments"),
        ));
    }
    Ok(())
}

fn expect_args<'a>(call: &'a syn::ExprCall, what: &str, n: usize) -> syn::Result<Vec<&'a syn::Expr>> {
    if call.args.len() != n {
        return Err(syn::Error::new_spanned(
            &call.func,
            format!("material DSL: `{what}` takes exactly {n} argument(s)"),
        ));
    }
    Ok(call.args.iter().collect())
}

/// Try to interpret a `material={...}` attribute value as a material
/// block. Returns `None` when the shape is not a material block (no
/// `.into_surface()` terminal) so the caller falls through to normal
/// expression semantics. Returns `Some(Err)` for loud rejections
/// (structural branching, unknown constructors, post passes).
pub(crate) fn material_block_value(attr: &syn::Expr) -> Option<syn::Result<TokenStream2>> {
    // The block form: one or more `let` bindings ending in a terminal
    // expression (`....into_surface()`). Anything else (single plain
    // expression, array sugar, chains without the terminal) is not a
    let (stmts, tail) = match attr {
        syn::Expr::Block(b) if b.attrs.is_empty() && b.label.is_none() => {
            let mut stmts: Vec<&syn::Stmt> = b.block.stmts.iter().collect();
            let tail = match stmts.pop() {
                Some(syn::Stmt::Expr(e, None)) => e.clone(),
                Some(syn::Stmt::Expr(e, Some(_))) => e.clone(),
                _ => return None,
            };
            (stmts, tail)
        }
        _ => return None,
    };
    // Terminal gate: the tail must be `.into_surface()` (possibly
    // wrapped in benign parens). No terminal → not a material block.
    if !is_into_surface(&tail) {
        return None;
    }
    Some(lower_block(stmts, &tail, None))}

/// Closure-param entry: `material={ |geo| ... }` — exactly one ident
/// param. The body unwraps to a block (or single tail expr); the geo
/// name threads into the interpreter so `geo.*` reads lower to
/// per-pixel host fields (D-M10a). Non-closure exprs return `None`
/// (not a material block — normal fall-through).
pub(crate) fn material_block_value_outer(attr: &syn::Expr) -> Option<syn::Result<TokenStream2>> {
    // Bare terminal call without a block: `material={Warp::sine(...).into_surface()}`
    // arrives UNbraced (rstml parses the braces as the attr delimiter, so
    // `attr_expr` unwraps the single tail out of its block). Same path as
    // the block form with zero `let`s. (The `|geo|` closure form below
    // keeps its own block handling — warp is flat/constant by nature.)
    if is_into_surface(attr) {
        return Some(lower_block(Vec::new(), attr, None));
    }    if let syn::Expr::Closure(c) = attr {
        // Zero-param closures (`move || stack`) are prebuilt dynamic values,
        // not the `|geo|` geometry form: fall through to normal expression
        // semantics instead of rejecting them (the interpreter only owns
        // the one-param `|geo| ...` shape).
        if c.inputs.is_empty() {
            return None;
        }
        if c.inputs.len() != 1 {
            return Some(Err(syn::Error::new_spanned(
                &c.inputs,
                "material DSL (Phase 2): `material={ |geo| ... }` takes exactly one geometry \
                 param (`|geo|`); the bare `material={ ... }` form means flat/constant geometry",
            )));
        }
        let syn::Pat::Ident(geo) = &c.inputs[0] else {
            return Some(Err(syn::Error::new_spanned(
                &c.inputs,
                "material DSL (Phase 2): the geometry param binds a plain name (`|geo| ...`)",
            )));
        };
        let geo_name = geo.ident.to_string();
        match c.body.as_ref() {
            syn::Expr::Block(b) if b.attrs.is_empty() && b.label.is_none() => {
                let mut stmts: Vec<&syn::Stmt> = b.block.stmts.iter().collect();
                let tail = match stmts.pop() {
                    Some(syn::Stmt::Expr(e, None)) => e.clone(),
                    Some(syn::Stmt::Expr(e, Some(_))) => e.clone(),
                    _ => {
                        return Some(Err(syn::Error::new_spanned(
                            &c.body,
                            "material DSL (Phase 2): `|geo| ...` holds `let` bindings + a terminal \
                             `.into_surface()` expression",
                        )));
                    }
                };
                if !is_into_surface(&tail) {
                    return None;
                }
                return Some(lower_block(stmts, &tail, Some(geo_name)));
            }
            // Single-expression body: `|geo| expr.into_surface()`.
            body => {
                if !is_into_surface(body) {
                    return None;
                }
                return Some(lower_block(Vec::new(), body, Some(geo_name)));
            }
        }
    }
    material_block_value(attr)
}

fn is_into_surface(e: &syn::Expr) -> bool {
    match e {
        syn::Expr::MethodCall(m) => m.method == "into_surface",
        syn::Expr::Paren(p) => is_into_surface(&p.expr),
        syn::Expr::Group(g) => is_into_surface(&g.expr),
        _ => false,
    }
}

fn lower_block(
    stmts: Vec<&syn::Stmt>,
    tail: &syn::Expr,
    geo: Option<String>,
) -> syn::Result<TokenStream2> {
    let mut interp = Interp::new();
    interp.geo_param = geo;
    // Phase 1: `let` bindings only (plus the terminal tail). Control
    // flow inside the block is structural branching — rejected loudly
    // (D-M6: `if` choosing closures cannot exist; value-level `if`
    // lives inside numeric arguments, which `numeric()` slots whole).
    for stmt in stmts {
        match stmt {
            syn::Stmt::Local(l) => {
                let syn::Pat::Ident(name) = &l.pat else {
                    return Err(syn::Error::new_spanned(
                        &l.pat,
                        "material DSL: `let` bindings bind a plain name (`let n = ...`)",
                    ));
                };
                if name.mutability.is_some() || name.by_ref.is_some() {
                    return Err(syn::Error::new_spanned(
                        &l.pat,
                        "material DSL: material bindings are immutable (`let n = ...`)",
                    ));
                }
                // Value fallback: `let g = hover;` and friends are
                // plain runtime expressions, not material shapes. Try
                // the material classifier first (its errors on material
                // shapes propagate loudly); non-material shapes become
                // verbatim value bindings.
                let Some(init) = l.init.as_ref() else {
                    return Err(syn::Error::new_spanned(
                        &l.pat,
                        "material DSL: `let n;` needs an initializer (`let n = ...`)",
                    ));
                };
                let binding = match interp.eval(&init.expr) {
                    Ok(b) => b,
                    Err(e) => {
                        if !is_material_shape(&init.expr) {
                            Binding::Value(init.expr.to_token_stream())
                        } else {
                            return Err(e);
                        }
                    }
                };
                interp.bindings.insert(name.ident.to_string(), binding);
            }
            syn::Stmt::Expr(_, _) | syn::Stmt::Item(_) | syn::Stmt::Macro(_) => {
                return Err(syn::Error::new_spanned(
                    stmt,
                    "material DSL (Phase 1): the block holds `let` bindings + a terminal \
                     `.into_surface()` expression — control flow here would be structural \
                     branching (fixed at compile time), which cannot vary per frame; \
                     put runtime `if` inside a numeric argument (one uniform slot)",
                ));
            }
        }
    }
    // Warp pad via the binding (not the IR): `eval_term` flattens
    // `WarpTerm` into the IR but the pad fact lives on the binding, so
    // re-resolve it here. A `let`-bound warp (`let w = Warp::sine(...);
    // w.into_surface()`) resolves through the bindings map; an inline
    // `Warp::sine(...).into_surface()` tail re-classifies directly.
    fn warp_pad_of(interp: &Interp, tail: &syn::Expr) -> Option<f32> {
    fn pad_literal(e: &syn::Expr) -> syn::Result<f32> { Interp::pad_literal(e) }
        // Strip the `.into_surface()` terminal to the receiver.
        let mut e = tail;
        loop {
            match e {
                syn::Expr::MethodCall(m) if m.method == "into_surface" => {
                    e = &m.receiver;
                }
                syn::Expr::Paren(p) => e = &p.expr,
                syn::Expr::Group(g) => e = &g.expr,
                _ => break,
            }
        }
        // `let`-bound path: look up the remembered binding.
        if let syn::Expr::Path(p) = e {
            if p.qself.is_none() && p.path.segments.len() == 1 {
                let name = p.path.segments[0].ident.to_string();
                if let Some(Binding::WarpTerm { pad, .. }) = interp.bindings.get(&name) {
                    return Some(*pad);
                }
            }
        }
        // Inline tail: it classified to a warp IR node — recover the pad
        // from the amplitude position (same `pad_literal` rule, including
        // the loud error for runtime amplitudes).
        if let syn::Expr::Call(call) = e {
            if let Some(fp) = func_path(call) {
                if fp == "Warp::sine" && call.args.len() == 2 {
                    return pad_literal(&call.args[1]).ok();
                }
            }
        }
        None
    }

    let term = interp.eval_term(tail)?;
    let lowered = crate::material_lower::lower_to_apply(&term)?;
    // Phase 4.5 surface facts: per-stage backdrop need + snapshot reach
    // (D-M8). Warp displaces sharp reads (`ReadSharp`, pad = amplitude);
    // every other v1 surface is an opaque plaque on the host card
    // (`None`, pad 0 — the backdrop enters only through `Refract`, which
    // Phase 1 rejects). The warp module bakes the literal amplitude and
    // frequency into WGSL (`Value::Lit` fast path in `warp_body_wgsl`):
    // the value, the pad fact, and the snapshot region all come from the
    // same compile-time literal, so they cannot drift. Per-frame variation
    // for warp comes from layering `.then(...)` chains and signal-driven
    // parents — never from per-slot filler the island subtree reads.
    let (need, pad_tokens) = match &term {
        MaterialExpr::Closure(crate::material_ir::Closure::Warp { .. }) => {
            match warp_pad_of(&interp, tail) {
                Some(pad) => (
                    quote! { ::riode::foundation::BackdropNeed::ReadSharp },
                    quote! { .with_pad(#pad) },
                ),
                None => {
                    return Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        "material DSL (Phase 4.5): warp amplitude must be an `Amplitude::px(<literal>)` \
                         so `Effect::pad` can size the snapshot region at compile time (D-M8); \
                         a runtime expression here would move displaced reads outside the \
                         captured region. Bind the amplitude as a literal.",
                    ));
                }
            }
        }
        _ => (
            quote! { ::riode::foundation::BackdropNeed::None },
            quote! {},
        ),
    };
    let slots = interp.arena.len() as usize;
    let filler = interp.filler;
    if filler.len() != slots {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "material DSL internal error: {} filler exprs for {} slots",
                filler.len(),
                slots
            ),
        ));
    }
    let preamble = interp.preamble;
    // Static vs Dynamic (the reactivity rule, §1.3): a block whose every
    // numeric is a literal bakes fully into source — emit the `EffectStack`
    // value directly (`IntoProp<EffectStack> for EffectStack` → static prop,
    // evaluated once). A block with uniform slots varies per frame (hover
    // `if`, signal reads, plain `.get()` calls — ALL of these live behind
    // uniform slots by D-M10b) — emit a `move || …` closure instead
    // (`IntoProp<EffectStack> for F: Fn() -> T` → `Prop::Dynamic` on the
    // host `GroupDef::material` path). The scheduler evaluates that closure
    // under the paint subscriber, so each signal the filler reads
    // subscribes; a hover write re-runs only the filler (new uniform
    // *values*, byte-identical WGSL source → pipeline cache hit per D-M5).
    // Emitting the stack directly here would read the signal once at mount
    // and freeze hover forever.
    //
    // Component boundary (`Option<EffectStack>` setter, `impl Into<_>`):
    // a `move || …` closure is NOT an `EffectStack` — the component prop
    // has no `Prop::Dynamic` form, so the dynamic branch cannot cross it.
    // A component that wants material reactivity takes `Prop<EffectStack>`
    // instead; the block author opts in by reading a signal inside
    // `material={...}` on a `Prop`-typed prop. The macro does NOT guess
    // the prop type — a dynamic block on a static prop is a loud error
    // naming the fix (this is the same honesty rule as every other
    // phase-gated rejection: never silently freeze hover).
    //
    // The literal-warp row (`material={Warp::sine(...).into_surface()}`
    // with literal params) lands in the static branch (slots == 0 → the
    // WARP_WGSL twin bakes with the literal amplitude — value and pad
    // come from the one literal, so they cannot drift) and crosses the
    // component boundary as a plain value.
    if slots == 0 {
        return Ok(quote! {
            {
                #(#preamble)*
                ::riode::foundation::EffectStack::from(::std::vec![
                    ::riode::foundation::BackdropEffect::into_backdrop_effect(
                        ::riode::foundation::CustomEffect::new(
                            #lowered,
                            ::std::vec![#(#filler as f32),*],
                            #need,
                        ) #pad_tokens
                    )
                ])
            }
        });
    }
    Ok(quote! {
        move || {
            #(#preamble)*
            ::riode::foundation::EffectStack::from(::std::vec![
                ::riode::foundation::BackdropEffect::into_backdrop_effect(
                    ::riode::foundation::CustomEffect::new(
                        #lowered,
                        ::std::vec![#(#filler as f32),*],
                        #need,
                    ) #pad_tokens
                )
            ])
        }
    })
}

/// Shapes the interpreter owns (errors on these always propagate):
/// known constructor paths, `*`/`+`, material method calls, `mix` /
/// `fresnel` / `Color` / `Sky` / `Intensity` positions. Everything else
/// (`if`, user-helper calls, field access, literals) is a value shape.
fn is_material_shape(e: &syn::Expr) -> bool {
    match e {
        syn::Expr::Call(call) => {
            let p = match call.func.as_ref() {
                syn::Expr::Path(p) => p,
                _ => return true,
            };
            let fp = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            matches!(
                fp.as_str(),
                "Normal::flat"
                    | "Sky::gain"
                    | "Microfacet::ggx"
                    | "Warp::sine"
                    | "Frequency::per_pixel"
                    | "Amplitude::px"
                    | "Color::rgb"
                    | "Color::white"
                    | "Color::red"
                    | "Color::green"
                    | "Color::blue"
                    | "Intensity::factor"
                    | "Intensity::lumens"
                    | "Refract::new"
                    | "Absorption::beer"
                    | "ThinFilm::interference"
                    | "fresnel"
                    | "mix"
            ) || {
                let head = p.path.segments.iter().next().map(|s| s.ident.to_string());
                matches!(
                    head.as_deref(),
                    Some("Microfacet")
                        | Some("Diffuse")
                        | Some("Sheen")
                        | Some("Conductor")
                        | Some("Emission")
                        | Some("Lens")
                        | Some("Bump")
                )
            }
        }
        syn::Expr::Binary(b) => matches!(b.op, syn::BinOp::Mul(_) | syn::BinOp::Add(_)),
        // Only the material terminal (and the Phase-2a `.frost` shape) are
        // material methods. Every other method call (`sig.get()`, helpers)
        // is a plain runtime value — falling through to a slot, never a
        // loud error here. (Unknown material *constructors* still error in
        // `eval_method` when the block IS material.)
        syn::Expr::MethodCall(m) => {
            m.method == "into_surface" || m.method == "frost" || m.method.to_string().starts_with("with_")
        }
        _ => false,
    }
}
