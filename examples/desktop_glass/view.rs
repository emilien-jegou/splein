// Single responsibility: five-island demo view (Phase 3a gate).
//
// Fullscreen transparent root with four glass islands (regular / clear /
// pill / bare-lens isolation) plus one plain semi-transparent card with
// NO effect.
// The engine paints glass content-only over transparent
// (`RIODE_COMPOSITOR_GLASS=1`, set by the driver): the compositor plugin
// owns the backdrop blur. The plain card proves exclusion by absence.
//
// The whole row drifts slowly (frame-clock driver) so island regions move
// every frame: the pool + the plugin mask track them reactively. Drift
// rides the `transform` prop (proven dynamic channel), applied once on
// the row — every island moves together.

use riode::foundation::Transform;
use riode::prelude::*;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release preview (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never mirrored.
/// The overlay host (`apps/desktop-glass`) sizes from layer-shell configure
/// and always paints transparent, ignoring HOT_BG (and HOT_WIDTH: the
/// five-island row wants ~1600px in the windowed preview).
pub const HOT_TITLE: &str = "Desktop glass — five islands";
pub const HOT_WIDTH: u32 = 1640;
pub const HOT_HEIGHT: u32 = 400;
pub const HOT_BG: u32 = 0x14161C;

#[component]
pub fn Page() -> impl IntoElement {
    let clock = use_frame_clock();
    let drift = clock.map(|secs| {
        let phase = (secs % 10.0) / 10.0;
        let tri = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
        (tri as f32 - 0.5) * 120.0
    });
    let shift = drift.map(|v| Transform::from_translation(v, 0.0));

    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            alignment="center"
            distribution="center"
            gap={48.0}
            transform={shift}
        >
            // Island 1: regular anodeui preset — the whole look in one
            // userland value, no engine fallbacks.
            <Group
                width={280.0}
                height={200.0}
                radius={24.0}
                backdrop_effects={GlassPreset::regular().style().into_effects()}
                alignment="center"
                distribution="center"
            >
                <Text size={16.0} weight={700u16} color={Color::WHITE}>{"REGULAR"}</Text>
            </Group>
            // Island 2: clear preset — sharp plateau, strong edge capture.
            <Group
                width={280.0}
                height={200.0}
                radius={28.0}
                backdrop_effects={GlassPreset::clear().style().into_effects()}
                alignment="center"
                distribution="center"
            >
                <Text size={16.0} weight={700u16} color={Color::WHITE}>{"CLEAR"}</Text>
            </Group>
            // Island 3: same preset as island 1, pill silhouette.
            <Group
                width={280.0}
                height={200.0}
                radius="max"
                backdrop_effects={GlassPreset::regular().style().into_effects()}
                alignment="center"
                distribution="center"
            >
                <Text size={16.0} weight={700u16} color={Color::WHITE}>{"LIQUID"}</Text>
            </Group>
            // Island 4 (control): plain semi-transparent fill, NO effect.
            // Must show the desktop sharp through 0.55 alpha — never blurred.
            <Group
                width={280.0}
                height={200.0}
                radius={28.0}
                fill={Color::hex_alpha(0x8899BB, 0.55)}
                alignment="center"
                distribution="center"
            >
                <Text size={16.0} weight={700u16} color={Color::WHITE}>{"PLAIN"}</Text>
            </Group>
            // Island 5 (isolation): bare clear lens — rim off, tone
            // linear, neutral stages. Predicts: no ring, no crush, a
            // sharp bent backdrop. Any surviving ring/crush names the
            // stage that still hides numbers.
            <Group
                width={280.0}
                height={200.0}
                radius={28.0}
                backdrop_effects={GlassPreset::clear_bare().style().into_effects()}
                alignment="center"
                distribution="center"
            >
                <Text size={16.0} weight={700u16} color={Color::WHITE}>{"BARE"}</Text>
            </Group>
        </Group>
    }
}
