// Single responsibility: Generic dynamic property abstraction supporting values, signals, and computations.

use crate::reactive::derived::Derived;
use crate::reactive::signal::Signal;
use std::rc::Rc;

/// Dynamic property holding either a static value or a reactive computation closure.
#[derive(Clone)]
pub enum DynamicProp<T> {
    Static(T),
    Dynamic(Rc<dyn Fn() -> T>),
}

impl<T: Clone + 'static> DynamicProp<T> {
    /// Evaluates the property value.
    pub fn evaluate(&self) -> T {
        match self {
            DynamicProp::Static(val) => val.clone(),
            DynamicProp::Dynamic(f) => f(),
        }
    }

    /// Whether this property encapsulates a reactive computation closure.
    #[inline(always)]
    pub fn is_dynamic(&self) -> bool {
        matches!(self, DynamicProp::Dynamic(_))
    }

    /// Extracts a clone of the dynamic closure handle, if dynamic.
    #[inline(always)]
    pub fn dynamic_closure(&self) -> Option<Rc<dyn Fn() -> T>> {
        match self {
            DynamicProp::Dynamic(f) => Some(Rc::clone(f)),
            DynamicProp::Static(_) => None,
        }
    }
}

/// Friendly alias for a reactive-capable component prop.
///
/// `Prop<String>` accepts a literal, `String`, `Signal<String>`,
/// `Derived<String>`, or a `move || ...` closure — anything implementing
/// [`IntoPropValue`]. `Option<Prop<T>>` means omissible; `Prop<Option<T>>`
/// means dynamically absent/present.
pub type Prop<T> = DynamicProp<T>;

/// Constructs a dynamic property from a computation closure.
pub fn computed<T, F: Fn() -> T + 'static>(f: F) -> DynamicProp<T> {
    DynamicProp::Dynamic(Rc::new(f))
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
    fn into_prop_value(self) -> DynamicProp<T>;
}

impl<T: Clone + 'static> IntoPropValue<T> for DynamicProp<T> {
    fn into_prop_value(self) -> DynamicProp<T> {
        self
    }
}

impl IntoPropValue<String> for &'static str {
    fn into_prop_value(self) -> DynamicProp<String> {
        DynamicProp::Static(self.to_string())
    }
}

impl IntoPropValue<String> for String {
    fn into_prop_value(self) -> DynamicProp<String> {
        DynamicProp::Static(self)
    }
}

impl IntoPropValue<f32> for f32 {
    fn into_prop_value(self) -> DynamicProp<f32> {
        DynamicProp::Static(self)
    }
}

impl IntoPropValue<bool> for bool {
    fn into_prop_value(self) -> DynamicProp<bool> {
        DynamicProp::Static(self)
    }
}

impl<T: Clone + 'static> IntoPropValue<T> for Signal<T> {
    fn into_prop_value(self) -> DynamicProp<T> {
        DynamicProp::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoPropValue<T> for Derived<T> {
    fn into_prop_value(self) -> DynamicProp<T> {
        DynamicProp::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoPropValue<Option<T>> for Option<T> {
    fn into_prop_value(self) -> DynamicProp<Option<T>> {
        DynamicProp::Static(self)
    }
}

impl<T: Clone + 'static, F: Fn() -> T + 'static> IntoPropValue<T> for F {
    fn into_prop_value(self) -> DynamicProp<T> {
        DynamicProp::Dynamic(Rc::new(self))
    }
}

/// Conversion trait into a generic dynamic property.
pub trait IntoProp<T> {
    /// Converts `self` into a `DynamicProp<T>`.
    fn into_prop(self) -> DynamicProp<T>;
}

impl<T> IntoProp<T> for DynamicProp<T> {
    fn into_prop(self) -> DynamicProp<T> {
        self
    }
}

impl IntoProp<bool> for bool {
    fn into_prop(self) -> DynamicProp<bool> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<f32> for f32 {
    fn into_prop(self) -> DynamicProp<f32> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<f32> for i32 {
    fn into_prop(self) -> DynamicProp<f32> {
        DynamicProp::Static(self as f32)
    }
}

impl IntoProp<f32> for u32 {
    fn into_prop(self) -> DynamicProp<f32> {
        DynamicProp::Static(self as f32)
    }
}

/// Numbers and bools render as text content: `text(12)`, `<Text>{count}</Text>`
/// via `view_child`, `label={true}` on a `Prop<String>` component arg.
macro_rules! impl_display_prop {
    ($($t:ty),*) => {
        $(
            impl IntoProp<String> for $t {
                fn into_prop(self) -> DynamicProp<String> {
                    DynamicProp::Static(self.to_string())
                }
            }
            impl IntoPropValue<String> for $t {
                fn into_prop_value(self) -> DynamicProp<String> {
                    DynamicProp::Static(self.to_string())
                }
            }
        )*
    };
}

impl_display_prop!(f32, i32, u32, usize, isize, bool);

impl IntoProp<i32> for i32 {
    fn into_prop(self) -> DynamicProp<i32> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<String> for String {
    fn into_prop(self) -> DynamicProp<String> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<String> for &'static str {
    fn into_prop(self) -> DynamicProp<String> {
        DynamicProp::Static(self.to_string())
    }
}

impl<T: Clone + 'static> IntoProp<T> for Signal<T> {
    fn into_prop(self) -> DynamicProp<T> {
        DynamicProp::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: Clone + 'static> IntoProp<T> for Derived<T> {
    fn into_prop(self) -> DynamicProp<T> {
        DynamicProp::Dynamic(Rc::new(move || self.get()))
    }
}

impl<T: 'static, F: Fn() -> T + 'static> IntoProp<T> for F {
    fn into_prop(self) -> DynamicProp<T> {
        DynamicProp::Dynamic(Rc::new(self))
    }
}
