// Single responsibility: Generic dynamic property abstraction supporting values, signals, and computations.

use crate::reactive::context::Ctx;
use crate::reactive::computed::Computed;
use crate::reactive::signal::Signal;
use std::rc::Rc;

/// A reactive-capable component prop: a static value or a live binding.
///
/// `Prop<String>` accepts a literal, `String`, `Signal<String>`, `Computed<String>`,
/// a `move || …` closure, or a `Ctx<T>` context read — anything implementing
/// [`IntoPropValue`]. `Option<Prop<T>>` means omissible; `Prop<Option<T>>` means
/// dynamically absent/present.
///
/// # The prop policy
///
/// One rule, applied everywhere a component takes a prop:
///
/// - **`T`** (plain) — a one-shot constructor value, reconciled per build.
/// - **`Prop<T>`** — a lazily-evaluated binding the component re-reads where
///   liveness matters (property positions that call `.evaluate()`).
///
/// Prefer plain `T` unless the prop genuinely needs to track a signal; mixed
/// signatures are fine *with the rule visible*.
#[derive(Clone)]
pub enum Prop<T> {
    Static(T),
    Dynamic(Rc<dyn Fn() -> T>),
    /// A lazily-resolved context read (see `reactive::context`). Resolves
    /// against the retained ancestor chain during node evaluation — never
    /// eagerly, because component bodies have no node yet.
    Context(Ctx<T>),
}

impl<T: Clone + 'static> Prop<T> {
    /// Evaluates the property value.
    ///
    /// A `Context` read resolves through the ambient evaluation node (set by
    /// reconcile, scheduler rebuilds, and handler dispatch); outside node
    /// evaluation it panics loudly rather than returning a stale guess.
    pub fn evaluate(&self) -> T {
        match self {
            Prop::Static(val) => val.clone(),
            Prop::Dynamic(f) => f(),
            Prop::Context(m) => m.resolve(),
        }
    }

    /// Whether this property encapsulates a reactive computation closure.
    ///
    /// Context reads count as dynamic: they subscribe (via the provider
    /// signal read under the observer) and must schedule like any binding.
    #[inline(always)]
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Prop::Dynamic(_) | Prop::Context(_))
    }

    /// Extracts a clone of the dynamic closure handle, if dynamic.
    ///
    /// Context markers resolve through a fresh closure capturing the marker:
    /// re-resolution on every call is what keeps scheduler rebuilds tracking
    /// provider updates.
    #[inline(always)]
    pub fn dynamic_closure(&self) -> Option<Rc<dyn Fn() -> T>> {
        match self {
            Prop::Dynamic(f) => Some(Rc::clone(f)),
            Prop::Context(m) => Some(m.resolving_closure()),
            Prop::Static(_) => None,
        }
    }
}

/// Constructs a dynamic property from a computation closure.
pub fn computed<T, F: Fn() -> T + 'static>(f: F) -> Prop<T> {
    Prop::Dynamic(Rc::new(f))
}

/// Default used when a `#[prop(default)]` prop is omitted from RSX.
///
/// Deliberately *not* `std::default::Default`: inferred defaults lie
/// (`bool::default()` is `false`, wrong for `visible`). Types opt in
/// explicitly; `Option<T>` defaults to `None`.
pub trait PropDefault {
    fn prop_default() -> Self;
}

impl<T> PropDefault for Option<T> {
    fn prop_default() -> Self {
        None
    }
}

/// Conversion into a reactive prop value.
///
/// Like [`IntoProp`] but named for the component-props use case: setters on
/// generated `*Props` builders take `impl IntoPropValue<T>` so literals,
/// signals, derivations, and closures all satisfy one bound.
pub trait IntoPropValue<T> {
    fn into_prop_value(self) -> Prop<T>;
}

impl<T: Clone + 'static> IntoPropValue<T> for Prop<T> {
    fn into_prop_value(self) -> Prop<T> {
        self
    }
}

impl IntoPropValue<String> for &'static str {
    fn into_prop_value(self) -> Prop<String> {
        Prop::Static(self.to_string())
    }
}

impl IntoPropValue<String> for String {
    fn into_prop_value(self) -> Prop<String> {
        Prop::Static(self)
    }
}

impl IntoPropValue<f32> for f32 {
    fn into_prop_value(self) -> Prop<f32> {
        Prop::Static(self)
    }
}

impl IntoPropValue<bool> for bool {
    fn into_prop_value(self) -> Prop<bool> {
        Prop::Static(self)
    }
}

impl<T: Clone + 'static> IntoPropValue<T> for Signal<T> {
    fn into_prop_value(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoPropValue<T> for Computed<T> {
    fn into_prop_value(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoPropValue<T> for Ctx<T> {
    fn into_prop_value(self) -> Prop<T> {
        Prop::Context(self)
    }
}

impl<T: Clone + 'static> IntoPropValue<Option<T>> for Option<T> {
    fn into_prop_value(self) -> Prop<Option<T>> {
        Prop::Static(self)
    }
}

impl<T: Clone + 'static, F: Fn() -> T + 'static> IntoPropValue<T> for F {
    fn into_prop_value(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(self))
    }
}

/// Conversion trait into a generic dynamic property.
pub trait IntoProp<T> {
    /// Converts `self` into a `Prop<T>`.
    fn into_prop(self) -> Prop<T>;
}

impl<T> IntoProp<T> for Prop<T> {
    fn into_prop(self) -> Prop<T> {
        self
    }
}

impl IntoProp<bool> for bool {
    fn into_prop(self) -> Prop<bool> {
        Prop::Static(self)
    }
}

impl IntoProp<f32> for f32 {
    fn into_prop(self) -> Prop<f32> {
        Prop::Static(self)
    }
}

impl IntoProp<f32> for i32 {
    fn into_prop(self) -> Prop<f32> {
        Prop::Static(self as f32)
    }
}

impl IntoProp<f32> for u32 {
    fn into_prop(self) -> Prop<f32> {
        Prop::Static(self as f32)
    }
}

/// Numbers and bools render as text content: `text(12)`, `<Text>{count}</Text>`
/// via `view_child`, `label={true}` on a `Prop<String>` component arg.
macro_rules! impl_display_prop {
    ($($t:ty),*) => {
        $(
            impl IntoProp<String> for $t {
                fn into_prop(self) -> Prop<String> {
                    Prop::Static(self.to_string())
                }
            }
            impl IntoPropValue<String> for $t {
                fn into_prop_value(self) -> Prop<String> {
                    Prop::Static(self.to_string())
                }
            }
        )*
    };
}

impl_display_prop!(f32, i32, u32, usize, isize, bool);

/// Integer literals into integer props (`count={7}` on a `Prop<usize>`): an
/// unsuffixed literal defaults to `i32`, and `IntoPropValue` is a *local* trait
/// so the cast is legal here (unlike `From<i32> for usize`, which the orphan
/// rule forbids). Mirrors `impl_num_prop!`'s i32/u32 sources.
macro_rules! impl_int_prop_value {
    ($($src:ty => $($dst:ty),*);* $(;)?) => {
        $(
            $(
                impl IntoPropValue<$dst> for $src {
                    fn into_prop_value(self) -> Prop<$dst> {
                        Prop::Static(self as $dst)
                    }
                }
            )*
        )*
    };
}

impl_int_prop_value! {
    i32 => u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize;
    u32 => u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize;
}

impl IntoProp<i32> for i32 {
    fn into_prop(self) -> Prop<i32> {
        Prop::Static(self)
    }
}

impl IntoProp<String> for String {
    fn into_prop(self) -> Prop<String> {
        Prop::Static(self)
    }
}

impl IntoProp<String> for &'static str {
    fn into_prop(self) -> Prop<String> {
        Prop::Static(self.to_string())
    }
}

impl<T: Clone + 'static> IntoProp<T> for Signal<T> {
    fn into_prop(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoProp<T> for Computed<T> {
    fn into_prop(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoProp<T> for Ctx<T> {
    fn into_prop(self) -> Prop<T> {
        Prop::Context(self)
    }
}

impl<T: 'static, F: Fn() -> T + 'static> IntoProp<T> for F {
    fn into_prop(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(self))
    }
}
