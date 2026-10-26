// Single responsibility: interpret the `view!` material block at expansion
// time (D-M6) into the `material_ir` tree, then lower it to a WGSL `apply`
// module and emit the runtime construction tokens.
//
// This is the single lowering path (D-M6 section 2): `backdrop={...}`
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
// - `.with_*` post passes are rejected loudly (deferred D-M2).
//
// Phase 1 subset (mirrors `material_lower`): `Normal::flat`,
// `Microfacet::ggx`, `Sky::gain` / `Color` / `Intensity` modulators,
// `fresnel` / scalar weights, `+` / `*` / `mix` composition. Refraction,
// Beer, ThinFilm, and the extended closures are rejected loudly with
// their gate phase — never a silent mis-lowering.

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
    Weight(Weight),
    /// Plain value binding: remembered tokens; uses allocate slots.
    /// (Slots allocate on first use, so emission order == fill order
    /// regardless of `let` order.)
    Value(TokenStream2),
}

/// Interpreter state for one `backdrop={...}` block.
struct Interp {
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
            arena: UniformArena::new(),
            bindings: HashMap::new(),
            filler: Vec::new(),
            preamble: Vec::new(),
            emitted: std::collections::HashSet::new(),
        }
    }

    /// A numeric position: float/int literal → `Lit` (baked into
    /// source); anything else → allocate a slot and remember the
    /// expression tokens for the filler (evaluated in arena order).
    fn numeric(&mut self, e: &syn::Expr) -> syn::Result<Value> {
        match e {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(f),
                ..
            }) => {
                let v: f32 = f.base10_parse().map_err(|_| {
                    syn::Error::new_spanned(f, "material DSL: float literal out of f32 range")
                })?;
                Ok(Value::Lit(v))
            }
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(i),
                ..
            }) => {
                // Integer in numeric position (e.g. `Sky::gain(1)`):
                // f32 literal — structure, baked into source.
                let v: f32 = i
                    .base10_parse::<i64>()
                    .map_err(|_| {
                        syn::Error::new_spanned(i, "material DSL: integer literal out of range")
                    })? as f32;
                Ok(Value::Lit(v))
            }
            syn::Expr::Path(p) if p.path.is_ident("true") => Ok(Value::Lit(1.0)),
            syn::Expr::Path(p) if p.path.is_ident("false") => Ok(Value::Lit(0.0)),
            other => {
                let slot = self.arena.alloc();
                // A slot referring to an earlier plain `let` binding
                // must see that binding's tokens: emit it into the
                // preamble once, before the constructor.
                if let syn::Expr::Path(p) = other {
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
            return Ok(Binding::Term(MaterialExpr::Modulated(
                Modulator::Color {
                    r: Value::Lit(r),
                    g: Value::Lit(g),
                    b: Value::Lit(b),
                },
                // Bare `Color` in term position tints the unit ambient
                // (a degenerate lobe); `*` with a real closure replaces
                // the inner term (factors attach inward).
                Box::new(MaterialExpr::Closure(Closure::Ggx {
                    normal: Normal::Flat,
                    roughness: Value::Lit(1.0),
                })),
            )));
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
        // Phase-gated shapes: reject loudly with the gate phase (never
        // a silent mis-lowering).
        if path_is(&p.path, &["Refract", "new"]) {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 1): `Refract` lowers in Phase 2a \
                 (displaced backdrop reads + frost blur) — use `Microfacet::ggx` / `Sky` / `Color` until then",
            ));
        }
        if path_is(&p.path, &["Absorption", "beer"]) {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 1): `Absorption::beer` lowers in Phase 2c \
                 (needs `path_length` from displaced reads)",
            ));
        }
        if path_is(&p.path, &["ThinFilm", "interference"]) {
            return Err(syn::Error::new_spanned(
                &call.func,
                "material DSL (Phase 1): `ThinFilm::interference` lowers in Phase 3 \
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
                        "material DSL (Phase 1): `{h}` lowers after Phase 1 (2a normals/Refract, \
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

    /// `a * b` (factors attach: Color/Intensity/Sky onto a term) and
    /// `a + b` (radiance sum) / `n1 + n2` (normal sum — Phase 2a shape,
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

    /// `.into_surface()` (terminal) and `.frost(r)` (Phase 2a shape,
    /// rejected loudly). `.with_*` post passes rejected (D-M2).
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
                 which lowers in Phase 2a — omit it until then",
            )),
            _ if name.starts_with("with_") => Err(syn::Error::new_spanned(
                &m.method,
                format!(
                    "material DSL: `.{name}(...)` is a deferred post pass (D-M2 — blocked on \
                     Step 5/6 RGBA16F post host); v1 surfaces end at `.into_surface()`"
                ),
            )),
            _ => Err(syn::Error::new_spanned(
                &m.method,
                format!(
                    "material DSL: unknown method `.{name}` — v1 knows `.into_surface()` \
                     (terminal) and `.frost` (Phase 2a, on `Refract`)"
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
            _ => Err(syn::Error::new_spanned(
                e,
                "material DSL: expected a material term here (a closure, `Color * ...`, `a + b`, `mix(...)`)",
            )),
        }
    }

    /// Weight position: `fresnel(n, ior)` (or a `let`-bound weight), else
    /// any scalar expression → one uniform slot (literals inline).
    fn eval_weight(&mut self, e: &syn::Expr) -> syn::Result<Weight> {
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
    if b.ident == "white" {
        Some((1.0, 1.0, 1.0))
    } else if b.ident == "red" {
        Some((1.0, 0.0, 0.0))
    } else if b.ident == "green" {
        Some((0.0, 1.0, 0.0))
    } else if b.ident == "blue" {
        Some((0.0, 0.0, 1.0))
    } else {
        None
    }
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

/// Try to interpret a `backdrop={...}` attribute value as a material
/// block. Returns `None` when the shape is not a material block (no
/// `.into_surface()` terminal) so the caller falls through to normal
/// expression semantics. Returns `Some(Err)` for loud rejections
/// (structural branching, unknown constructors, post passes).
pub(crate) fn material_block_value(attr: &syn::Expr) -> Option<syn::Result<TokenStream2>> {
    // The block form: one or more `let` bindings ending in a terminal
    // expression (`....into_surface()`). Anything else (single plain
    // expression, array sugar, chains without the terminal) is not a
    // material block — fall through.
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
    Some(lower_block(stmts, &tail))
}

fn is_into_surface(e: &syn::Expr) -> bool {
    match e {
        syn::Expr::MethodCall(m) => m.method == "into_surface",
        syn::Expr::Paren(p) => is_into_surface(&p.expr),
        syn::Expr::Group(g) => is_into_surface(&g.expr),
        _ => false,
    }
}

fn lower_block(stmts: Vec<&syn::Stmt>, tail: &syn::Expr) -> syn::Result<TokenStream2> {
    let mut interp = Interp::new();
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
    let term = interp.eval_term(tail)?;
    let lowered = crate::material_lower::lower_to_apply(&term)?;
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
    // evaluated once). A block with uniform slots may vary per frame (hover
    // `if`, signal reads) — emit a `move || …` closure instead
    // (`IntoProp<EffectStack> for F: Fn() -> T` → `Prop::Dynamic`). The
    // scheduler evaluates that closure under the paint subscriber, so each
    // signal the filler reads subscribes; a hover write re-runs only the
    // filler (new uniform *values*, byte-identical WGSL source → pipeline
    // cache hit per D-M5). Emitting the stack directly here would read the
    // signal once at mount and freeze hover forever.
    if slots == 0 {
        return Ok(quote! {
            {
                #(#preamble)*
                ::riode::foundation::EffectStack::from(::std::vec![
                    ::riode::foundation::BackdropEffect::into_backdrop_effect(
                        ::riode::foundation::CustomEffect::new(
                            #lowered,
                            ::std::vec![#(#filler as f32),*],
                            ::riode::foundation::BackdropNeed::None,
                        )
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
                        ::riode::foundation::BackdropNeed::None,
                    )
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
        syn::Expr::MethodCall(_) => true,
        _ => false,
    }
}
