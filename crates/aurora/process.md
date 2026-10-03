# Aurora Motion API Specification

This document provides a complete showcase of the Aurora Motion API through focused micro-examples, progressing from basic state transitions to advanced physics takeovers.

---

## 1. Identity & View Anchoring

Aurora separates **ephemeral layout descriptions** from **retained identity and visual state**.

### 1.1 Stable View Identity
Animations target a retained `ViewId` rather than a temporary builder struct.

```rust
// Create or lookup a stable retained view handle
let panel = view("inspector-panel");

// Attach it to the declarative tree
column()
    .view(panel.clone())
    .width(320.0)
    .fill_height()
```

### 1.2 Layout vs. Compositor Transform Separation
Layout properties (`width`, `gap`, `padding`) compute static bounds. Motion properties (`x`, `y`, `scale`, `rotation`, `opacity`) run directly on the compositor without triggering reflows.

```rust
// Static structural layout (runs once or on resize)
card.layout()
    .width(280.0)
    .padding(16.0);

// GPU compositor transforms (runs at 60/120Hz)
card.motion()
    .translate_x(40.0)
    .scale(1.05)
    .opacity(0.9);
```

---

## 2. Declarative Transitions

For simple state changes where UI elements animate in response to state toggles.

### 2.1 Basic Property Transition
Interpolates values when reactive dependencies change.

```rust
button()
    .opacity(if is_enabled.get() { 1.0 } else { 0.4 })
    .scale(if is_hovered.get() { 1.02 } else { 1.0 })
    .animate(200.ms(), Ease::OutCubic)
```

### 2.2 Inline Spring State
Uses physics-based resting instead of fixed durations.

```rust
badge()
    .scale(if is_selected.get() { 1.15 } else { 1.0 })
    .spring(Spring::snappy())
```

---

## 3. Imperative Multi-Property Tweens

A strongly typed, property-bag animation engine.

### 3.1 Multi-Property Target
Animates multiple properties concurrently under a single controller.

```rust
panel.animate()
    .to()
    .x(360.0)
    .y(0.0)
    .scale(0.96)
    .opacity(0.0)
    .duration(300.ms())
    .ease(Ease::OutQuint)
    .play();
```

### 3.2 Spring-Driven Target
Springs serve as dynamic physical simulations with phase space $(x, v)$, rather than pre-baked easing curves.

```rust
modal.animate()
    .to()
    .scale(1.0)
    .opacity(1.0)
    .spring(Spring {
        stiffness: 400.0,
        damping: 28.0,
        mass: 1.0,
        velocity: 0.0,
    })
    .play();
```

### 3.3 Seamless Interruption
Retargeting an in-flight motion samples the instantaneous position and velocity without visual popping or hitching.

```rust
// 1. Initial trigger: Spring to x = 300
button.animate().to().x(300.0).spring(Spring::snappy()).play();

// 2. Interrupted 80ms later: Retargets to x = 100
// Samples current (position, velocity) and creates a continuous new curve
button.animate().to().x(100.0).spring(Spring::snappy()).play();
```

### 3.4 Playback Controls
Controllers can be paused, resumed, reversed, or scrubbed.

```rust
let motion = drawer.animate().to().x(0.0).duration(400.ms());

motion.play();
motion.pause();
motion.resume();
motion.reverse();
motion.seek(0.5); // Seek to 50%
motion.cancel();
```

---

## 4. Timeline Orchestration & Choreography

Orchestrates multi-view sequences using both absolute and relative timing markers.

### 4.1 Relative Sequence Positioning
Uses relative timing tokens to build coordinated choreographies without hardcoded duration calculations.

```rust
timeline()
    .label("dialog-enter")
    
    // Animate backdrop first
    .to(&backdrop)
        .opacity(1.0)
        .duration(200.ms())

    // Start panel spring simultaneously with backdrop ("<")
    .to(&panel)
        .scale(1.0)
        .spring(Spring::snappy())
        .at("<")

    // Stagger title 50ms after the panel starts ("+50ms")
    .to(&title)
        .y(0.0)
        .opacity(1.0)
        .ease(Ease::OutCubic)
        .at("+50ms")

    // Run metrics at the exact same moment as title ("<")
    .to(&metrics)
        .opacity(1.0)
        .scale(1.0)
        .at("<")
        
    .play();
```

### 4.2 Absolute Timestamp Sequencing
Schedules actions at explicit millisecond offsets on a unified timeline.

```rust
timeline()
    .at(0.ms(),   hero.animate().y(0.0).opacity(1.0))
    .at(80.ms(),  subtitle.animate().opacity(1.0))
    .at(140.ms(), cta_button.animate().scale(1.0).spring(Spring::bouncy()))
    .play();
```

---

## 5. Reactive Signal-Driven Motion

Binds motion values into the reactive graph, driving arbitrary properties as mathematical functions of physical movement.

### 5.1 Spring Signals Driving Compound Properties
One physical spring simulation drives multiple visual properties simultaneously.

```rust
// Returns a continuous Signal<f32> representing spring progress (0.0 -> 1.0)
let progress: Signal<f32> = Spring::snappy().into_signal();

panel
    .x(progress.map(|p| p * 360.0))
    .opacity(progress.map(|p| p.clamp(0.0, 1.0)))
    .scale(progress.map(|p| 0.95 + (p * 0.05)));
```

### 5.2 Continuous Spatial Sampling
Properties can be derived continuously from spatial position rather than from time elapsed.

```rust
let x_pos = backdrop.motion().x();

// Color is sampled from a spatial palette based on current physical position x(t)
backdrop.fill(x_pos.map(|x| {
    palette.sample_color_at(x)
}));
```

---

## 6. Gestures & Direct Manipulation

Connects touch and pointer gestures directly to the motion layer, preserving momentum across interaction boundaries.

### 6.1 Direct Manipulation with Momentum Handoff
Translates views directly during drags, then transfers touch velocity into a spring on release.

```rust
card
    .gesture(Gesture::drag().axis(Axis::Horizontal))
    .on_drag(|drag| {
        // Direct, unlagged GPU translation
        drag.view().motion().translate_x(drag.translation().x);
    })
    .on_drag_end(|drag| {
        // Feed user's release velocity directly into the settle spring
        drag.view()
            .motion()
            .animate()
            .x(0.0)
            .spring(
                Spring::snappy()
                    .velocity(drag.velocity().x)
            )
            .play();
    });
```

### 6.2 Gesture-Mapped Interpolation
Binds interactive drag distances to range-mapped signals.

```rust
let swipe = gesture(&drawer).horizontal().range(-300.0..0.0);

// Directly map drag distance to backdrop dimming
backdrop.opacity(swipe.progress().map_range(0.0..1.0, 0.0..0.7));
```

---

## 7. Automatic Layout Transitions (FLIP Engine)

Structural container layout changes (insert, remove, reorder) run FLIP transitions automatically behind the scenes.

### 7.1 Declarative Layout Transitions
Modifying reactive state automatically snapshots layout coordinates before and after, springing the delta back to zero.

```rust
column()
    .gap(8.0)
    // Automatically springs children from old layout rect to new layout rect
    .layout_transition(LayoutTransition::spring(Spring::snappy()))
    .children(
        items.get().iter().map(|item| {
            card_view(item).key(item.id) // Keyed identity enables FLIP tracking
        })
    )
```

### 7.2 Presence-Driven Collapse & Expand
Views can dynamically alter their layout footprint without unmounting or losing their state.

```rust
// Animate Presence: Present -> Absent collapses layout height automatically
row.layout_presence(if is_collapsed {
    LayoutPresence::Absent
} else {
    LayoutPresence::Present
});
```

---

## 8. Motion Takeover & Ballistic Physics

Views can detach from the layout hierarchy, transition into a free-body physics simulation, and reattach cleanly.

### 8.1 Detach with Layout Collapse (`Detach::Collapse`)
Detaches an element into screen space while neighboring items smoothly collapse the vacated spot.

```rust
button("Yeet")
    .on_click(|btn| {
        let card = btn.view().parent();

        card.take_motion()
            // 1. Layout marks card as Absent; list collapses shut via FLIP
            .detach(Detach::Collapse)
            // 2. Impart ballistic impulses
            .velocity(Vec2::new(450.0, -800.0))
            .angular_velocity(8.0)
            // 3. Enter rigid-body physics simulation
            .physics(
                Physics::new()
                    .gravity(2000.0)
                    .restitution(0.70)
                    .friction(0.80)
            )
            .bounds(Bounds::screen()) // Bounce off display edges
            .play();
    })
```

### 8.2 Momentum-Preserving Reattachment
Brings a detached, floating physical body back into its layout slot without position or velocity pops.

```rust
card.motion().reattach(
    // 1. Layout marks card as Present; list expands to make room
    // 2. Motion creates a spring from the current screen-space (x, y, v)
    //    straight into the calculated layout target
    Reattach::spring(Spring::smooth())
);
```

### 8.3 Detach with Reservation (`Detach::KeepSpace` / `Placeholder`)
Takes over visual motion while preserving the layout slot (e.g., standard card dragging).

```rust
card.take_motion()
    // Keeps layout footprint intact so neighboring elements do not shift
    .detach(Detach::KeepSpace)
    .translate_to(pointer_coords);
```

---

## 9. Complete Architectural Overview

```text
                           Declarative View Tree
                        (column / row / stack / text)
                                     │
                                     ▼
                              Stable ViewId
                         (Identity & Lifecycle)
                                     │
                ┌────────────────────┴────────────────────┐
                ▼                                         ▼
           Layout Layer                              Motion Layer
     (Width / Height / Bounds)              (Transform / Opacity / Filters)
                │                                         │
        LayoutPresence                                    │
  (Present / Absent / Placeholder)                        │
                │                                         │
                ▼                                         ▼
      Layout Transactions / FLIP                  Motion Ownership
  (Before Snapshot -> After Snapshot)        ┌────────────┼────────────┐
                │                            │            │            │
                ▼                         Tween        Spring       Physics
     Delta Offset to Spring                  │            │            │
                │                            └────────────┼────────────┘
                │                                         │
                └────────────────────┬────────────────────┘
                                     ▼
                            Frame Synchronization
                           (Central Frame Clock)
                                     │
                                     ▼
                           Compositor Transforms
                                (GPU Draw)
```

