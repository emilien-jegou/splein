# Motion in Aurora

How to animate things, and why the engine does less of it than you might expect.

The short version: **Aurora's engine lays out and paints. It does not decide how anything gets
from one place to another.** Every transition is a value you own, held in a signal, integrated
over time. If a view should grow, you animate the number it grows by. If content inside that view
should move with it, that content reads the same number.

That single rule is what makes the awkward cases work. Most animation bugs in a retained-tree UI
come from two parts of a view disagreeing about a value — the box being animated by the engine
while the text inside it jumped to its final position on the same frame. Here that cannot happen,
because there is only one number and both read it.

---

## Contents

1. [The model](#the-model)
2. [Animating a value](#1-animating-a-value)
3. [Animating a whole box](#2-animating-a-whole-box)
4. [Things the engine will not do for you](#3-things-the-engine-will-not-do-for-you)
5. [Reading where an element ended up](#4-reading-where-an-element-ended-up)
6. [FLIP: animating a reorder](#5-flip-animating-a-reorder)
7. [Following a pointer](#6-following-a-pointer)
8. [Rebuilds: why `use_transition` exists](#7-rebuilds-why-use_transition-exists)
9. [Interruption and retargeting](#8-interruption-and-retargeting)
10. [Self-driving playback](#9-self-driving-playback)
11. [Springs and tweens](#10-springs-and-tweens)
12. [Debugging motion](#11-debugging-motion)
13. [Testing motion](#12-testing-motion)
14. [Rules worth internalising](#rules-worth-internalising)

---

## The model

Three pieces, and it is worth being precise about which is which:

| | |
|---|---|
| **Signals** | Plain values. `Signal<T>` holds one, notifies subscribers when it is written. |
| **Transitions** | A `Signal<f32>` that chases another signal over time. You write the target; you read the current value. |
| **Layout** | Given the numbers you declared, works out where everything goes. Instantaneous. No interpolation, no easing, no spring. |

The engine also has a fixed-step clock. Every transition integrates on the same grid
(`FIXED_STEP_SECS = 1/120`), so an animation lands in the same place at 30fps as at 144fps. You
never interact with the grid directly; it exists so your animation is frame-rate independent
without you thinking about it.

Nothing in the engine takes a `Duration` and applies it to a property. That is the whole design.

---

## 1. Animating a value

The primitive. `Transition::on` takes anything readable, returns a signal holding the value *as it
animates*.

```rust
use aurora::prelude::*;

#[component]
fn Toggle() -> impl IntoElement {
    let on = use_signal(false);

    // 0.0 -> 1.0, eased, whenever `on` flips.
    let progress = Transition::ease(180.ms(), Ease::OutCubic)
        .on(on.clone().map(|on| if on { 1.0 } else { 0.0 }));

    group()
        .opacity(progress)
        // ...or anywhere else that takes a signal.
        .children([text(progress.map(|p| format!("{:.0}%", p * 100.0)))])
        .into_element()
}
```

Two constructors:

- `Transition::ease(duration, ease)` — fixed duration, an easing curve. Right when the motion has
  a natural length and the end matters more than the middle.
- `Transition::spring(spring)` — physics. Right when the motion can be interrupted, reversed, or
  given a velocity from a gesture.

`Transition::on` returns the **current** value, so it composes with everything: drive a
`width`, an `opacity`, a `scale`, or a string.

Modifiers:

```rust
Transition::spring(Spring::snappy())
    .from(0.0)          // start somewhere other than the current value
    .to(1.0)            // and finish somewhere other than the target signal
    .repeat(Repeat::PingPong)
```

---

## 2. Animating a whole box

This is where the model pays off, and where most of the real bugs used to live.

**Drive the size from a signal, and derive everything inside from that same signal.**

```rust
// Wrong: the box animates, but this text re-evaluates instantly.
group()
    .width(expanded.map(|e| if e { 196.0 } else { 52.0 }))
    .children([text(expanded.map(|e| if e { "Validate" } else { "Go" }))])

// Right: one animated value, read by both.
let width = Transition::spring(Spring::snappy())
    .on(expanded.clone().map(|e| if e { 196.0 } else { 52.0 }));

group()
    .width(width.clone())
    .children([text(width.map(|w| if w > 124.0 { "Validate" } else { "Go" }))])
```

In the first version the label swaps to `"Validate"` on frame one while the box is still 52px
wide, so for ~200ms you get a long word laid out in a box that has not grown yet. In the second,
the label swaps when the box is actually wide enough — because "when the box is wide enough" and
"what the box's width is" are the same number.

This is the pattern to reach for whenever a container changes size. There is no separate "reveal"
mechanism to configure, and nothing to keep in sync, because the content is laid out against the
real size on every frame.

An accordion item is exactly this:

```rust
let height = Transition::spring(Spring::smooth())
    .on(is_open.clone().map(|open| if open { OPEN } else { HEADER }));

group()
    .height(height)
    .clip(true)
    .children([header(...), body(...)])
```

---

## 3. Things the engine will not do for you

There is no `.animate()`, no `.layout_motion()`, no FLIP pass. If you are porting from an engine
that has them, here is what to do instead:

| You want | Do this |
|---|---|
| Animate a property | Animate a signal, pass it as the property. |
| Animate a layout size | Animate the size signal. Layout runs per frame on the real value. |
| Reveal content as a box grows | Nothing. The box is a real size, so the content is laid out and clipped by it. |
| FLIP a reorder | `use_flip` — [section 5](#5-flip-animating-a-reorder). |
| Follow a pointer | `use_follow` — [section 6](#6-following-a-pointer). |

The reason the engine does not interpolate layout for you is that it cannot do it *safely*: a
child that reads the same animated value as its parent's box has to be re-laid out every frame,
which is real work, and doing it behind your back means paying it whether or not anything moves.

---

## 4. Reading where an element ended up

Sometimes you need to know where something actually is. `use_ref` gives you a handle.

```rust
use aurora::dsl::use_ref;

#[component]
fn Row() -> impl IntoElement {
    let me = use_ref();
    group().bind_ref(me).into_element()
}
```

```rust
me.rect()        // Option<ResolvedRect>, in window coordinates
me.position()    // Option<Point>, top-left
me.center()      // Option<Point>
me.size()        // Option<Point>
me.child_rects() // Vec<ResolvedRect>
```

Two things to know:

**It is a pull, not a push.** Nothing happens when the geometry changes. If you want to react, you
sample — from a gesture, from a ticker, or after a layout you already know happened. This is
deliberate: a read cannot dirty a node, so it cannot schedule the frame that would invalidate the
answer, and therefore cannot oscillate. If you find yourself wanting the measurement to *notify*
you, you probably want an animated signal instead ([section 1](#1-animating-a-value)).

**It is one frame behind.** The geometry published is the frame that just laid out. That is the
price of sampling rather than reacting, and it is the right trade.

---

## 5. FLIP: animating a reorder

FLIP is four steps: **F**irst position, **L**ast position, **I**nvert (jump back without
animating), **P**lay (animate that offset to zero). `use_flip` does all four and returns the
transform to apply.

```rust
#[component]
fn Row(index: usize, spec: RowSpec) -> impl IntoElement {
    let row_ref = use_ref();

    let flip = use_flip(
        row_ref.clone(),
        spec.drag.revision.map(Trigger::from),
        Spring::snappy(),
    );

    row(index, &spec.label).bind_ref(row_ref).transform(flip).into_element()
}
```

### Why the trigger is not optional

`use_flip`'s second argument is a `Trigger` you bump whenever **the arrangement** changes — not
whenever the position changes.

This matters more than it looks. Squashing the window moves every row in the list. If FLIP keyed
off position alone, every resize would look like a shuffle, animating rows that nobody reordered.
The trigger separates "this row swapped places" from "the whole list got shorter", so a resize
re-lays out instantly and only a real reorder animates.

```rust
// A revision rather than the order itself: comparing five labels per row per frame is work
// nobody asked for, when a single counter says the same thing.
let revision = use_signal(0u32);

fn move_row(&self, from: usize, to: usize) {
    let mut next = self.order.get();
    let moved = next.remove(from);
    next.insert(to, moved);
    self.order.set(next);
    self.revision.set(self.revision.get() + 1);   // <- this is what FLIP watches
}
```

### The held-row rule

A dragged row is already being told where to go — by the pointer. It must not *also* carry a FLIP
offset toward the slot the reorder moved it into. Two commands on one axis make the row lag a
stride behind the cursor and then catch up.

Gate on the hold, not on "the offset is non-zero" — every row binds both offsets unconditionally,
so keying off a non-zero value disables the FLIP everywhere:

```rust
let is_held = drag.held.map({ let label = label.clone();
    move |held| held.as_deref() == Some(label.as_str()) });

let flip = flip.combine(is_held, |flip, held| {
    if held { Transform::IDENTITY } else { flip }
});
```

---

## 6. Following a pointer

`use_follow` keeps an element at a requested vertical position, measured against its own current
box rather than accumulated from the grab.

```rust
let follow = use_follow(row_ref.clone(), want_centre, grab_dy);
```

- `want_centre` — `Option<f32>`: where the row's centre should sit, or `None` to let it rest.
- `grab_dy` — the pointer's displacement at the moment of the grab.

It returns a `Signal<Transform>`, so it composes with `use_flip` the same way any two signals do:

```rust
fn offset(flip: Derived<Transform>, follow: Signal<Transform>) -> Derived<Transform> {
    flip.combine(follow, |a, b| Transform::from_translation(a.tx + b.tx, a.ty + b.ty))
}
```

A sum, not a matrix product, because both write a y translation and neither writes an x. Said out
loud because a general compose would be wrong here in a way that only shows up as diagonal drift.

---

## 7. Rebuilds: why `use_transition` exists

A reactive list re-runs its builder whenever its contents change. That is a feature — it is how a
reorder reconciles — but it means anything you allocate in a builder is allocated again.

`Transition::on` seeds its output at the target's current value. For a one-shot that is exactly
right. For a row in a reactive list it is wrong in a specific, nasty way: the rebuilt row gets a
**brand-new** spring already sitting on its new slot, so there is no distance to travel and the
animation that should be a FLIP is a teleport.

```rust
use aurora::reactive::hooks::use_transition;

let index = order.map(move |o| position_of(&o, &label) as f32);

// Recalls the cell this component used last time, so the row keeps its position *and* its
// velocity across the rebuild, and simply retargets.
let animated = use_transition(Transition::spring(Spring::snappy()), index);
let flip = animated.combine(index, |a, i| a - i);
```

The same reasoning applies to `use_ref`: it recalls the handle across rebuilds so a rebuilt row is
still the same row.

> `use_transition` lives in `aurora::reactive::hooks` and is not currently re-exported from the
> crate root or the prelude. If you want it in everyday use, that export is a one-line change.

---

## 8. Interruption and retargeting

Springs retarget for free, and that is the main reason to prefer them over tweens for anything a
user can touch.

```rust
let width = Transition::spring(Spring::snappy())
    .on(target);   // write `target` mid-flight and it turns around from where it is
```

A spring keeps its live velocity across a target change, so a flick that is caught mid-flight
continues smoothly instead of restarting.

To hand a gesture's velocity to a spring on release:

```rust
release_lift.set_with_velocity(0.0, info.velocity.y);
```

Tweens restart their segment from the current value when the target changes, so interruption is
also correct for them — just without the momentum.

---

## 9. Self-driving playback

`Transition::on` needs a target signal to watch. Sometimes the animation is its own source of
truth — a progress bar, an intro sequence, something with a play button.

```rust
let (progress, control) = use_playback(Transition::ease(400.ms(), Ease::InOut), 0.0, 1.0);

control.pause();
control.resume();
control.restart();
control.seek(0.5);          // fraction
let state = control.state(); // PlayState
let out = control.output();
```

---

## 10. Springs and tweens

```rust
Transition::ease(duration, ease)   // Tween::new(duration, ease)
Transition::spring(spring)        // Spring
```

Presets, so you are not tuning three numbers to get "normal":

```rust
Spring::smooth()   // settled, no overshoot — layout, disclosure
Spring::snappy()   // quick, slight overshoot — rearrangement
Spring::bouncy()   // playful overshoot — feedback, delight
Spring::new(stiffness, damping, mass)
Spring::snappy().with_velocity(v)   // start moving
```

Easings, in full: `Linear`, `InCubic`, `OutCubic` (the default), `InOutCubic`, `OutQuint`,
`InOutQuint`, `OutExpo`.

Durations are seconds. `Millis` is sugar for readability — `180.ms()` — and works on both `f32`
and `u32`, so `180.ms()` and `180.0.ms()` are both fine. `Repeat` is `Once` (default), `PingPong`,
or `Loop`.

---

## 11. Debugging motion

With the debug overlay enabled (`F10` toggles the HUD):

| Key | Effect |
|---|---|
| `F8` | Freeze / resume time |
| `.` | Advance exactly one 1/120s quantum while frozen |
| `[` `]` | Step the time scale through `0.05x … 1x` |
| `S` | Write `/tmp/aurora-frame-N.png` plus a display-list dump |
| `F9` `F11` `F12` | Hierarchy, layout bounds, damage heatmap |

Freezing and then stepping with `.` is the reason the freeze key exists, and the HUD names both
keys while time is not running normally.

Headlessly, the same thing:

```rust
h.pause();      // freeze
h.step_once();  // advance one quantum
h.settle(300);  // run until nothing is moving, up to a limit
```

---

## 12. Testing motion

`aurora::testing::Harness` drives an engine without a window.

```rust
let mut h = Harness::root(page(), 400, 400);
h.step();
assert!(h.settle(300));
let mid = h.located("card").center();
```

The assertions that catch real bugs:

**Track the value, not the frame count.** A spring's exact position after N frames is a property
of the integrator; what you care about is that it got closer, that it settled, and that a held row
stays under the cursor. Assert those.

**Measure geometry; never import a declared constant.** A declared size is an input; the measured
box is the output, after shrink and after fill distribution. A test that asserts against the
declaration is asserting that layout did nothing.

```rust
// No: ROW is what the demo asked for
assert!((h.located("row").flow.height - reorder::ROW).abs() < 0.5);

// Yes: this is what layout actually gave it
let pitch = h.located("row-b").flow.y - h.located("row-a").flow.y;
```

**Reorder assertions should name which rows moved**, not just that something moved — otherwise
they pass whenever the whole list shuffles.

---

## Rules worth internalising

1. **One value drives the box and its content.** If both read it, they cannot disagree.
2. **Animate the real layout value.** There is no cheaper engine-side path, and a second box is
   a second source of truth.
3. **Never mix two writers on one axis.** A pointer offset and a FLIP offset on the same row must
   be gated against each other.
4. **Position is not a reason to animate.** A resize moves everything; a trigger distinguishes
   "rearranged" from "resized".
5. **State that must survive a rebuild goes in hook memory** (`use_ref`, `use_transition`), not in
   a builder local.
6. **Read geometry; don't subscribe to it.** A read cannot cause the loop that a reaction can.
7. **Check that content fits.** A child that does not fit its parent is shrunk, and nothing in a
   screenshot says so. It reads as "the animation looks slightly wrong".