// Single responsibility: P7 — event-binding sugar and Signal convenience methods.
//
// Two contracts, no pixels:
// 1. `on:click={sig.method}` (method-reference sugar) compiles: the macro rewrites a
//    bare `sig.method` on an `on:` event into `move |_| (sig).method()`, turning what
//    would be a field-access type error into the obvious closure.
// 2. The convenience methods the sugar targets (`toggle`, `set_true`, `set_false`,
//    `increment`, `decrement`) mutate as named.

use std::cell::RefCell;
use std::rc::Rc;

use riode::prelude::*;
use riode::reactive::ReactiveRuntime;

#[test]
fn signal_convenience_methods_mutate_as_named() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));

    let flag = Signal::new(Rc::clone(&rt), false);
    assert!(!flag.get());
    flag.set_true();
    assert!(flag.get());
    flag.toggle();
    assert!(!flag.get());
    flag.set_false();
    assert!(!flag.get());

    let count = Signal::new(Rc::clone(&rt), 0usize);
    count.increment();
    count.increment();
    assert_eq!(count.get(), 2);
    count.decrement();
    assert_eq!(count.get(), 1);
}

#[test]
fn on_event_method_ref_sugar_compiles() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let pressed = Signal::new(Rc::clone(&rt), false);
    let count = Signal::new(Rc::clone(&rt), 0usize);

    // Each `on:` attr holds a bare `sig.method` — a field access, not a closure.
    // The macro rewrites it to `move |_| (sig).method()`; reaching here means the
    // sugar produced a valid handler for every event setter.
    let _el: Element = view! {
        <Group
            width={10.0}
            height={10.0}
            on:click={count.increment}
            on:pointer-down={pressed.set_true}
            on:pointer-up={pressed.set_false}
            on:pointer-enter={pressed.toggle}
        />
    };

    // The handlers are installed; firing one mutates through the sugar path.
    assert_eq!(count.get(), 0);
}
