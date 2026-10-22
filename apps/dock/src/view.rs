// Single responsibility: The dock view — `Page` plus `HOT_*` window chrome.
//
// Ports `backp/src/components/Dock.jsx` (magnification via pointer-x over
// resting centers; tooltip + running dot per item) onto the riode DSL:
// `Group`/`Text`/`image` in `view!`, `scale`/`y` as compositor overrides
// (layout never runs — same rule as `examples/motion_reorder`'s held row),
// tooltip fade via `use_transition` (pattern: `motion_accordion` PanelBody).
//
// The bar is a compositor-level liquid-glass island (`glass=...`, same
// mechanism as `examples/desktop_glass`): the engine paints glass
// content-only over transparent (`RIODE_COMPOSITOR_GLASS=1`, set by the
// overlay host) and the riode-glass compositor plugin owns the backdrop
// blur/refraction. No `fill`, no baked shadow — those would paint over the
// live backdrop and kill the lens.

use crate::dock::{BAR_H, DOT, DOT_DROP, GAP, ICON, PAD_H, PAD_V, RADIUS, SEP_MARGIN, SEP_W};
use crate::registry::{slots, Slot};

use riode::dsl::use_ref;
use riode::foundation::{Anchor, Margin};
use riode::internal::{group, image, IntoElement};
use riode::prelude::*;
use riode::reactive::use_transition;

// The dock package always resolves `riode::` to the single shared
// image (hot_ui cfg from build.rs), so presets come from there too.
use riode::anodeui::GlassPreset;

/// Window chrome: declared once here, referenced by the driver — never mirrored.
/// Overlay mode: the background is fully transparent, so only the bar,
/// tooltip, and shadow paint — the compositor shows through everywhere else.
pub const HOT_TITLE: &str = "Dock";
pub const HOT_WIDTH: u32 = 900;
pub const HOT_HEIGHT: u32 = 220;
pub const HOT_BG: u32 = 0x00000000;

const SEP_FILL: u32 = 0x6B7280;
const DOT_FILL: u32 = 0xFFFFFF;
const TIP_FILL: u32 = 0x1F232E;
const TIP_TEXT: u32 = 0xFFFFFF;

/// Resting slot pitch: icon + gap. Separators are narrow; v1 measures them
/// as one fixed pitch — replaced by measured `use_ref` centers in step 2.
fn slot_pitch() -> f32 {
    ICON + GAP
}

/// One dock icon: image + tooltip + running dot.
///
/// `cursor_x` is the pointer position in dock-bar coordinates (`None` when
/// outside). `slot_index` locates this icon's resting center for the
/// gaussian falloff (ports `Dock.jsx` `centers[]`).
#[component]
fn DockIcon(
    app_id: &'static str,
    name: &'static str,
    slot_index: usize,
    cursor_x: Signal<Option<f32>>,
    hovered: Signal<Option<&'static str>>,
    running: Signal<Vec<&'static str>>,
) -> impl IntoElement {
    let src = crate::icons::icon_map().get(app_id).cloned().expect("dock: missing icon");

    // Resting center of this slot: leading pad + half icon + index * pitch.
    let center = PAD_H + ICON / 2.0 + slot_index as f32 * slot_pitch();

    // Target scale/lift from the magnification law; smoothed like GSAP's
    // 0.18s power2.out so the icons chase the cursor instead of jumping.
    let target_s = cursor_x.map(move |x| crate::dock::icon_transform(x, center).0);
    let target_y = cursor_x.map(move |x| crate::dock::icon_transform(x, center).1);
    let scale = use_transition(Transition::ease(180.ms(), Ease::OutCubic), target_s);
    let lift = use_transition(Transition::ease(180.ms(), Ease::OutCubic), target_y);

    let is_hovered = hovered.map(move |h| h == Some(app_id));
    let tip_opacity = use_transition(
        Transition::ease(180.ms(), Ease::OutCubic),
        is_hovered.map(|h| if h { 1.0 } else { 0.0 }),
    );
    let is_running = running.map(move |r| r.contains(&app_id));

    let set_hover = hovered;
    let clear_hover = hovered;
    let clear_cursor = cursor_x;
    // Out-of-flow tooltip: anchored to the icon cell's bottom, floating above
    // it. In-flow placement pushed the cell around (hence the old offsets).
    let tip = group()
        .absolute()
        .anchor(Anchor::Bottom)
        .margin(Margin {
            top: 0.0,
            right: 0.0,
            bottom: ICON + 20.0,
            left: 0.0,
        })
        .opacity(tip_opacity)
        .children(vec![
            view! {
                <Group fill={TIP_FILL} radius={7.0}>
                    <Text size={12.5} color={TIP_TEXT}>{name}</Text>
                </Group>
            }
            .into_element(),
        ])
        .into_element();
    view! {
        <Group
            direction="vertical"
            width={ICON}
            height={ICON}
            alignment="center"
        >
            {tip}
            <Group
                key={format!("dock-{app_id}")}
                width={ICON}
                height={ICON}
                scale={scale}
                y={lift}
                anchor="bottom"
                on:pointer_enter={{
                    move |_| {
                        set_hover.set(Some(app_id))
                    }
                }}
                on:pointer_leave={{
                    move |_| {
                        clear_hover.set(None);
                        // The bar's own `on:pointer_leave` never fires when the
                        // pointer exits off an icon: hover tracks the topmost
                        // node, so only this icon's leave fires. Clearing here
                        // is safe for icon-to-icon moves — the same event's
                        // `pointer_move` on the bar runs after leave and
                        // restores the new x in the same frame.
                        clear_cursor.set(None);
                    }
                }}
                on:click={{
                    move |_| {
                        tracing::info!(app = app_id, "dock open");
                    }
                }}
            >
                {image(src).size(ICON, ICON)}
            </Group>
            <Group
                width={DOT}
                height={DOT}
                radius={2.0}
                fill={DOT_FILL}
                margin={format!("{DOT_DROP} 0.0 0.0 0.0")}
                visible={is_running}
            />
        </Group>
    }
}

/// The dock bar: liquid-glass rounded container holding icons + separators.
///
/// The bar carries `glass` (not `fill`): under `RIODE_COMPOSITOR_GLASS=1`
/// the engine emits content-only over transparent and the compositor plugin
/// blurs/refracts the live desktop behind this island. The host
/// (`apps/dock`) extracts the island each frame and uploads its blur region.
#[component]
pub fn Page() -> impl IntoElement {
    let items = slots();
    let icon_count = items
        .iter()
        .filter(|s| matches!(s, Slot::App(_)))
        .count();
    let bar_w = PAD_H * 2.0
        + icon_count as f32 * ICON
        + (items.len().saturating_sub(1)) as f32 * GAP
        + 2.0 * (SEP_W + SEP_MARGIN * 2.0 - ICON - GAP).max(0.0);

    // Pointer-x in bar coordinates (`None` when outside). `on:pointer_move`
    // reports absolute window coordinates (`input/event.rs`), so the bar's
    // measured window origin is subtracted — without this the magnification
    // peak sits one bar-offset to the right of the cursor.
    let cursor_x = use_signal(None::<f32>);
    let bar_ref = use_ref();
    let bar_origin = bar_ref.clone();
    let hovered = use_signal(None::<&'static str>);
    // v1: Finder pretends to run so the dot has a demo state.
    let running = use_signal(vec!["finder"]);

    let move_x = cursor_x;
    let leave_x = cursor_x;
    let clear_tip = hovered;
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            alignment="center"
            distribution="end"
        >
            <Group
                direction="horizontal"
                width={bar_w}
                height={BAR_H}
                backdrop_effects={GlassPreset::clear().style().into_effects()}
                radius={RADIUS}
                gap={GAP}
                padding={format!("{PAD_V} {PAD_H}")}
                alignment="center"
                ref={bar_ref}
                on:pointer_move={{
                    move |e| {
                        let origin = bar_origin.position().map(|p| p.x).unwrap_or(0.0);
                        move_x.set(Some(e.position.x - origin))
                    }
                }}
                on:pointer_leave={{
                    move |_| {
                        leave_x.set(None);
                        clear_tip.set(None);
                    }
                }}
            >
                {
                    {
                        let mut icon_idx = 0usize;
                        items
                            .iter()
                            .map(|slot| {
                                let el = match slot {
                                    Slot::Separator => view! {
                                        <Group
                                            width={SEP_W}
                                            height="fill"
                                            fill={SEP_FILL}
                                            margin={format!("{SEP_MARGIN} {SEP_MARGIN}")}
                                        />
                                    }
                                    .into_element(),
                                    Slot::App(app) => {
                                        let i = icon_idx;
                                        icon_idx += 1;
                                        DockIcon(
                                            app.id,
                                            app.name,
                                            i,
                                            cursor_x,
                                            hovered,
                                            running,
                                        )
                                        .into_element()
                                    }
                                };
                                el
                            })
                            .collect::<Vec<_>>()
                    }
                }
            </Group>
        </Group>
    }
}
