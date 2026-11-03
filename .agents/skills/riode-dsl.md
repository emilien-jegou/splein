# Riode UI Framework: Declarative DSL Specification (`dsl.md`)

This is the definitive reference for the Riode declarative UI language (RSX and builder API). Riode is an analytic Signed Distance Field (SDF) framework targeting WebGPU (`wgpu`). It does not wrap HTML/DOM.

---

## 1. Module Imports & Crate Structure

In Riode, visual primitives (`Fill`, `Shape`, `Glass`, `Blur`, `Shadow`, `Blend`) are imported from **`riode_shader`**. Foundation types and layout spacing (`Color`, `Padding`, `Margin`, `BlendMode`, `Effect`, `Anchor`, `Size`) are imported from **`riode::prelude::*`**.

```rust
use riode::dsl::IntoElement;
use riode::prelude::*;
use riode_shader::{Fill, Shape, Glass, Blur, Shadow, Effect, Blend};
```

---

## 2. Returning & Storing Elements in Variables

The `view!` macro translates RSX into nested builder method chains (`GroupDef`, `TextDef`, component builders, etc.).

### Storing Fragments
Every `view!` block evaluates to an **`Element`** enum variant (`Element::Group`, `Element::Text`, etc.), which implements **`IntoElement`**.
```rust
let custom_header = view! {
    <Group padding={Padding::all(8.0)}>
        <Text>"Header Title"</Text>
    </Group>
};

view! {
    <Group direction="vertical">
        {custom_header}
        <Text>"Body Content"</Text>
    </Group>
}
```

---

## 3. Reactive Prop System & Binding Rules

All properties in Riode accept literals, static values, reactive signals (`Signal<T>`), derived computations (`Computed<T>`), or closures.

### Prop Value Matrix
| Syntax in `view!` | Resolved Type | Observer Trigger |
| :--- | :--- | :--- |
| `width={100.0}` | `Prop::Static(Size::Fixed(100.0))` | None |
| `width="fill"` | `Prop::Static(Size::Fill)` | None |
| `material={Fill::solid(0xFFFFFF)}` | `Prop::Static(Material)` | None |
| `material={my_signal}` | `Prop::Dynamic(Rc<dyn Fn() -> T>)` | Subscribes **Paint Observer** |
| `width={my_width_signal}` | `Prop::Dynamic(Rc<dyn Fn() -> T>)` | Subscribes **Layout Observer** |

*Optimization*: Layout observers invalidate measurement/geometry. Paint observers invalidate shaders/colors/uniforms. Changing a button's color or material never triggers layout recalculation.

---

## 4. Components & Control Flow

### Components (`#[component]`)
Components are ordinary Rust functions annotated with `#[component]`. They generate a builder struct (`{Name}Props`) used by RSX:
```rust
#[component(name = "my_app::Card")]
pub fn Card(title: String, children: Vec<Element>) -> impl IntoElement {
    view! {
        <Group shape={Shape::rounded(12.0)} material={Fill::solid(0x313244)}>
            <Text>{title}</Text>
            {children}
        </Group>
    }
}
```

### Control Flow (`<if>`, `<for>`, `<match>`)
Control nodes splice directly into parent layouts with zero carrier wrapper nodes.

#### Conditional Branching (`<if>` / `<else>`)
```rust
view! {
    <Group direction="vertical">
        <if {*is_active.get()}>
            <Text>"Active"</Text>
        <else>
            <Text>"Inactive"</Text>
        </if>
    </Group>
}
```

#### Keyed Collections (`<for>`)
```rust
view! {
    <Group direction="vertical">
        <for each={items.get()} key={|item| item.id}>
            {|index, item| view! {
                <Group width="fill" height={40.0}>
                    <Text>{item.title}</Text>
                </Group>
            }}
        </for>
    </Group>
}
```
*Rule*: Always provide a stable, position-independent `key` so reconciliation diffing preserves component hook state across list reorders.

#### Pattern Matching (`<match>`)
```rust
view! {
    <Group direction="vertical">
        <match {*auth_state.get()}>
            <case pat={AuthState::LoggedOut}>
                <Text>"Please sign in"</Text>
            </case>
            <case pat={AuthState::LoggedIn(id)}>
                <Text>{format!("User #{id}")}</Text>
            </case>
        </match>
    </Group>
}
```

---

## 5. Container Visual Model: The 4 Properties

Every layout container (`<Group>`) supports four distinct visual properties. All accept a single item or a collection list (`[...]`).

```rust
<Group
    shape={Shape::squircle(24.0)}
    material=[
        Shadow::drop(16.0, 8.0),
        Glass::frosted(12.0),
        Fill::solid(0xFF8800).blend(BlendMode::ColorDodge),
    ]
    overlay=[
        GoldBorderOverlay,
    ]
    effect={[
        Effect::wave(10.0, 5.0),
        BlendMode::Screen,
    ]}
>
    <Text>"Content"</Text>
</Group>
```

### Execution Pipeline & Differences

| Prop | Execution Phase | Reads | Purpose & Operational Details |
| :--- | :--- | :--- | :--- |
| **`shape`** | Setup | Geometry / SDF | Defines mathematical boundaries as analytic Signed Distance Fields (SDF) with unit gradient ($|\nabla d| = 1$). Writes scalar distance to `FragmentPayload.dist` (`dist < 0` inside, `dist > 0` outside). Anti-aliasing is subpixel algebraic filtering (`clamp(0.5 - dist, 0.0, 1.0)`). Allocates 0 MSAA buffers. <br>• `Shape::rect()`<br>• `Shape::rounded(16.0)`<br>• `Shape::squircle(24.0)` |
| **`material`** | Before Children | Backdrop + Shape | Renders what the container is *made of*. Evaluates `payload.dist` and accumulates **premultiplied linear RGBA** color into `payload.color`. Executes *before* children draw. Can sample underlying backdrop for glass/refraction. Re-collecting a list allocates an immutable `Arc` chain ($O(1)$ pointer-diffing). <br>• `Fill::solid(0x313244)`<br>• `Glass::frosted(12.0)`<br>• `Shadow::drop(16.0, 8.0)` |
| **`overlay`** *(opt)* | After Children | Children + Shape | Renders *after* children have drawn, but is strictly **clipped to the container's `shape`** using subpixel anti-aliasing. Ideal for inner borders, mouse hover highlights, focus rings, or sheens. |
| **`effect`** | Layer Composite | Flattened Subtree | Acts as a *lens* over the container and everything inside it. **Fused into a single pass** (zero intermediate offscreen passes). Executes stages sequentially (e.g., UV distortion $\rightarrow$ color grading $\rightarrow$ backdrop blend). |

---

## 6. Shader-Level Blending (`BlendMode`)

Blending is computed in WGSL shaders, enabling non-linear modes (Overlay, SoftLight, ColorDodge, ColorBurn, HardLight, etc.) without fixed-function GPU blend state permutations.

### Supported Modes
`Normal`, `Multiply`, `Screen`, `Overlay`, `Darken`, `Lighten`, `ColorDodge`, `ColorBurn`, `HardLight`, `SoftLight`, `Difference`, `Exclusion`, `Hue`, `Saturation`, `Color`, `Luminosity`, `Additive`.

### Usage Contexts
1. **Surface Blending**: Attach `.blend(BlendMode::...)` to any material or overlay stage. Blends against the accumulated result of earlier stages in the stack (`payload.color`), falling back to backdrop if empty.
   ```rust
   Fill::solid(0xFF8800).blend(BlendMode::ColorDodge)
   ```
2. **Subtree / Effect Blending**: Pass a `BlendMode` directly into `effect[...]` to set how a flattened subtree mixes into its parent backdrop. Evaluated in authored order.
   ```rust
   <Group effect={BlendMode::Multiply}> ... </Group>
   <Group effect={[Effect::wave(10.0, 5.0), BlendMode::Screen]}> ... </Group>
   ```
3. **Leaf Element Blending**: Text, Images, and SVGs accept a `blend` property.
   ```rust
   <Text blend={BlendMode::Additive}>"120 FPS"</Text>
   <Svg graphic={icon} blend={BlendMode::Overlay} />
   ```

---

## 7. Leaf Elements & Event Handlers

- **`<Text>`**: Typography run with wrapping, selection, and decoration. Supports `.blend(BlendMode::...)`.
- **`<Image>`**: Raster bitmap asset.
- **`<Svg>`**: Scalable vector graphic. Supports `.blend(BlendMode::...)`.

### Event Handlers (`on:`)
```rust
<Group
    on:click={|info| println!("Clicked at {:?}", info.point)}
    on:pointer-down={|e| press.set(true)}
    on:drag-start={|info| ...}
    on:wheel={|info| ...}
>
    <Text>"Interactive"</Text>
</Group>
```
*Method reference sugar*: `on:click={state.increment}` expands to `move |_| (state).increment()`.

---


Here is the rewritten and expanded **Section 8** for `dsl.md`. It explicitly covers custom **`Material`** stage authoring alongside `Overlay` and `EffectStage`, and details exactly how `Geo` is used to pack runtime measurements into GPU uniform buffers.

---

## 8. Custom Stage Authoring

Developers can define custom shaders without touching core engine internals. Riode supports three stage interfaces: **Material Stages**, **Overlay Stages**, and **Effect Stages**.

### General Rules
- WGSL snippets can declare private helper functions, structs, and constants (automatically scoped/prefixed by the compiler to prevent namespace collisions).
- Uniform buffers must be **16-byte aligned** (`vec4<f32>` chunks).
- Use **`Geo`** inside `write_uniforms` to compute geometry-dependent parameters (like dynamic stroke widths, border radii proportional to element size, or retina pixel scaling via `geo.dpr`).

---

### A. Custom Material Stage (`Stage`)
Renders before children, reading backdrop and shape distance (`payload.dist`).
```rust
use riode::foundation::{Color, Geo, Stage};

#[derive(Clone, Copy, Debug)]
pub struct DynamicBorderStage {
    pub color: Color,
}

pub const BORDER_WGSL: &str = r#"
fn stage_apply(payload: ptr<function, FragmentPayload>, uniforms: vec4<f32>) {
    let thickness = uniforms.x;
    let dist_stroke = abs((*payload).dist) - (thickness * 0.5);
    let cov = clamp(0.5 - dist_stroke, 0.0, 1.0) * clamp(0.5 - (*payload).dist, 0.0, 1.0);
    
    let col = vec4<f32>(uniforms.y, uniforms.z, uniforms.w, uniforms.x); // color payload
    let src = col * cov;
    (*payload).color = src + (*payload).color * (1.0 - src.a);
}
"#;

impl Stage for DynamicBorderStage {
    fn wgsl_source(&self) -> &'static str { BORDER_WGSL }
    fn uniform_size(&self) -> usize { 16 } // 16-byte aligned vec4<f32>
    
    fn write_uniforms(&self, geo: &Geo, buffer: &mut [u8]) {
        // Compute thickness proportional to box dimensions using Geo
        let thickness = geo.width.min(geo.height) * 0.05;
        buffer[0..4].copy_from_slice(&thickness.to_ne_bytes());
        buffer[4..8].copy_from_slice(&self.color.r.to_ne_bytes());
        buffer[8..12].copy_from_slice(&self.color.g.to_ne_bytes());
        buffer[12..16].copy_from_slice(&self.color.b.to_ne_bytes());
    }
}
```

---

### B. Custom Overlay Stage (`Stage`)
Renders after children have drawn, strictly clipped to the container's `shape`. Shares the exact `Stage` trait as material stages.
```rust
#[derive(Clone, Copy, Debug)]
pub struct GoldBorderOverlay;

impl Stage for GoldBorderOverlay {
    fn wgsl_source(&self) -> &'static str {
        r#"
        fn stage_apply(payload: ptr<function, FragmentPayload>) {
            // Guard exterior pixels (using > 0.5 to preserve the 1px subpixel AA band)
            if ((*payload).dist > 0.5) { return; }
            let border_dist = abs((*payload).dist) - 2.0;
            let cov = clamp(0.5 - border_dist, 0.0, 1.0) * clamp(0.5 - (*payload).dist, 0.0, 1.0);
            let src = vec4<f32>(1.0, 0.84, 0.0, 1.0) * cov;
            (*payload).color = src + (*payload).color * (1.0 - src.a);
        }
        "#
    }
    fn uniform_size(&self) -> usize { 0 }
    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
}
```

---

### C. Custom Effect Stage (`EffectStage`)
Post-processes the flattened subtree layer when composited to the parent.
```rust
#[derive(Clone, Copy, Debug)]
pub struct InvertEffect;

impl EffectStage for InvertEffect {
    fn wgsl_source(&self) -> &'static str {
        r#"
        fn effect_apply_color(src_col: vec4<f32>, px: vec2<f32>, geo: ElementGeo) -> vec4<f32> {
            if (src_col.a <= 0.0) { return src_col; }
            let rgb = src_col.rgb / src_col.a;
            return vec4<f32>((vec3<f32>(1.0) - rgb) * src_col.a, src_col.a);
        }
        "#
    }
    fn uniform_size(&self) -> usize { 0 }
    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
}

---

## 9. Advanced Reactive State & Memory Hooks

To manage component lifecycles, local variables, and animation without rebuilding components on every frame, Riode provides a fine-grained reactive runtime.

### Signals (`Signal<T>`)
Mutable reactive cells. Reading a signal (`sig.get()`) registers the current observer; writing to it (`sig.set(val)`) marks downstream computations and observers dirty and schedules a frame tick.
```rust
let count = use_signal(|| 0);
// Read: count.get()
// Write: count.set(count.get() + 1);
```

### Computed Values (`Computed<T>`)
Derived reactive values. Lazily evaluated, memoized, and auto-invalidating when dependency signals fire.
```rust
let double = use_computed(move || count.get() * 2);
```

### Component Local Hooks (`use_ref`, `use_signal`)
Component bodies run **exactly once per mount**—never on every animation frame. Reactive bindings push live values directly to the GPU or layout engine every tick.
- **`use_signal(|| initial)`**: Allocates state scoped to the component instance.
- **`use_ref()`**: Binds an `ElementRef` handle to a node so you can poll its physical window rect (`ref.rect()`, `ref.center()`, `ref.size()`) in a non-reactive pull manner without triggering layout invalidation loops.

---

## 10. Motion & Physics Gestures

Riode includes built-in spring physics and gesture handlers that decouple event mechanics from view layouts.

### FLIP Animation (`use_flip`)
Animates elements between layout position changes smoothly using spring physics.
```rust
let target_ref = use_ref();
let (transform_sig, flip_handle) = use_flip(
    target_ref.clone(),
    trigger_signal,
    Spring::stiff(),
);

view! {
    <Group
        transform={transform_sig}
        bind_ref={target_ref}
    >
        <Text>"Movable Row"</Text>
    </Group>
}
```

### Drag Gestures (`Drag` / `use_gesture`)
Encapsulates press, hold, track, and release lifecycle into a single verb.
```rust
let drag_pos = use_gesture();
view! {
    <Group
        x={drag_pos.map(|p| p.map_or(0.0, |pt| pt.x))}
        y={drag_pos.map(|p| p.map_or(0.0, |pt| pt.y))}
        on:drag={Drag::start(drag_pos)
            .grab(|e| println!("Grabbed"))
            .step(|info| println!("Dragging delta: {:?}", info.delta))
            .end(|info| println!("Released with velocity: {:?}", info.velocity))}
    >
        <Text>"Drag Me"</Text>
    </Group>
}
```

---

## 11. Context Provision and Consumption

Contexts allow dependency injection down the view tree without prop-drilling.

### Providing Context (`Provider`)
```rust
let theme_signal = use_signal(|| Theme::Dark);

view! {
    <Provider grant={ThemeKey.provide(theme_signal)}>
        <ThemedCard />
    </Provider>
}
```

### Consuming Context
Inside any descendant component or builder:
```rust
let current_theme = ThemeKey.expect(); // reads nearest ancestor grant
```
