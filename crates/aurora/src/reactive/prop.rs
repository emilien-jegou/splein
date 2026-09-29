// Single responsibility: Generic dynamic property abstraction supporting values, signals, and computations.

use std::rc::Rc;
use crate::reactive::derived::Derived;
use crate::reactive::signal::Signal;

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

/// Constructs a dynamic property from a computation closure.
pub fn computed<T, F: Fn() -> T + 'static>(f: F) -> DynamicProp<T> {
    DynamicProp::Dynamic(Rc::new(f))
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

impl IntoProp<f32> for f32 {
    fn into_prop(self) -> DynamicProp<f32> {
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
