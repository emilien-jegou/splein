// Single responsibility: Crate root declaring the public domain module boundaries.

// Lets `#[component]`'s expansion resolve. The macro emits absolute `::aurora::` paths, which
// only work from a downstream crate; without this alias they do not resolve here, so the macro
// was unusable inside the crate itself and in-crate components had to be hand-rolled instead.
// Declaring the crate as its own `aurora` makes one expansion valid on both sides of the
// boundary, so the macro no longer has to guess which side it is on.
extern crate self as aurora;

pub mod anim;
pub mod app;
pub mod dsl;
pub mod foundation;
pub mod input;
pub mod layout;
pub mod pipeline;
pub mod prelude;
pub mod reactive;
pub mod render;
pub mod runtime;
pub mod scene;
pub mod text;
pub mod tree;
