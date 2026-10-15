// Single responsibility: P6 — bare integer literals reach component props.
//
// An unsuffixed integer literal defaults to `i32`. The generated `*Props`
// setters used to be `impl Into<T>`, and `From<i32> for usize` cannot exist
// (orphan rule), so `len={100_000}` failed to compile and forced `len={100_000usize}`
// ceremony the repr DSL never needs. Integer props now take a *concrete*
// setter (matching primitives like `weight(u16)`), so the literal infers the
// prop's own type. Reactive integer props (`Prop<usize>`) gain bare-literal
// support via `IntoPropValue<T> for i32`/`u32`.
//
// This is a compile-pass contract: if a setter regresses to `impl Into<T>`,
// the bare literals below stop compiling.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{group, Component, Element, IntoElement};
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::{Cx, ReactiveRuntime};

#[component]
fn Plain(len: usize, index: Option<u16>) -> impl IntoElement {
    let _ = (len, index);
    group().width(10.0).height(10.0)
}

#[component]
fn Reactive(count: Prop<usize>) -> impl IntoElement {
    let _ = count;
    group().width(10.0).height(10.0)
}

#[test]
fn bare_integer_literals_reach_component_props() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let el: Element = cx.enter(|| {
        view! {
            <Group width={10.0}>
                <Plain len={100_000} index={3} />
                <Reactive count={7} />
            </Group>
        }
    });
    // Building the tree is the whole contract — reaching here means the bare
    // literals typechecked against usize / Option<u16> / Prop<usize>.
    let _ = el;
}
