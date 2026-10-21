// Single responsibility: glow-blend-mode comparison — three glass cards
// sharing ONE identical emissive wash, differing only in the wash's
// blend mode against the glass base (`Glow::blend`). Isolation is the
// point: a mid-tone split with no backdrop art, so any colour proves
// the emissive path. Human judges which mode reads as the card's own
// light; no test asserts.
//
// The split is mid-light/mid-dark, NOT white/dark: Overlay and
// SoftLight preserve the backdrop's luminance and collapse to it at
// the extremes (spec: at Cb=0.5 Overlay returns the source exactly;
// near Cb=0 or 1 it vanishes). A near-white/near-black surround —
// which the spill era used — makes Overlay invisible by construction.
// Mid-tones keep every mode visible; the two halves still prove the
// wash against both a lighter and a darker surround.
//
// Left → right: Normal (plain source-over, the pre-blend reference —
// the wash sits on the slab like paint), Overlay (print overprint:
// multiply over dark glass, screen over light — the wash darkens and
// brightens the slab it lands on), SoftLight (the wash bleeds into the
// slab like a wash of ink, gentle at both ends). The wash geometry must
// render identically on all three — only the composite against the
// base differs; that sameness is the assertion the eyes check.

use riode::foundation::{Anchor, BlendMode, Color, GlassStyle, Glow, GlowAnchor};
use riode::internal::group;
use riode::prelude::*;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Glow Blend";
pub const HOT_WIDTH: u32 = 720;
pub const HOT_HEIGHT: u32 = 520;
// Dark surround, not white: theatre mode replays the (bright) card onto
// the backdrop, and blurred-white on white is invisible by construction —
// no gain staging can fix that without inventing colour. Dark is the
// halo's natural habitat (cf. liquid-glass-faithful's own #0f1012 page);
// the interior wash reads even better against it. If the halo is still
// absent on dark, the renderer pass itself is broken — that distinction
// is the point of this change.
pub const HOT_BG: u32 = 0x141619;

// The shared interior: the reference 6-stop palette (amber → orange-red →
// pink → purple → blue → cyan), bottom-anchored. ONE definition, cloned
// into all three cards — the demo's whole point is that only the blend
// mode varies.
fn shared_glow(blend: BlendMode) -> Glow {
    Glow::new()
        .stops([
            Color::hex(0xFFB340),
            Color::hex(0xFF6A3D),
            Color::hex(0xFF3D84),
            Color::hex(0xB04DFF),
            Color::hex(0x4F7DFF),
            Color::hex(0x36D3FF),
        ])
        .anchor(GlowAnchor::Bottom)
        .span(0.70)
        .strength(1.2)
        .inside(0.9)
        .refracted(true)
        .blend(blend)
}

#[component]
pub fn Page() -> impl IntoElement {
    // Transparent `clear` lens: minimal wash/frost/saturation stages, so
    // almost nothing stands between the emissive term and the eye.
    let normal: GlassStyle = GlassPreset::clear_bare()
        .style()
        .with_glow(shared_glow(BlendMode::Normal));
    let overlay: GlassStyle = GlassPreset::clear_bare()
        .style()
        .with_glow(shared_glow(BlendMode::Overlay));
    let soft: GlassStyle = GlassPreset::clear_bare()
        .style()
        .with_glow(shared_glow(BlendMode::SoftLight));

    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            clip={true}
            alignment="center"
            distribution="center"
        >
            {bg_half(0x9DA0A4, true)}
            {bg_half(0x45484C, false)}
            <Group direction="horizontal" gap={28.0} alignment="center">
                <Group
                    width={200.0}
                    height={150.0}
                    radius={24.0}
                    effects={normal.into_effects()}
                    alignment="center"
                    distribution="center"
                    z_index={2}
                />
                <Group
                    width={200.0}
                    height={150.0}
                    radius={24.0}
                    effects={overlay.into_effects()}
                    alignment="center"
                    distribution="center"
                    z_index={2}
                />
                <Group
                    width={200.0}
                    height={150.0}
                    radius={24.0}
                    effects={soft.into_effects()}
                    alignment="center"
                    distribution="center"
                    z_index={2}
                />
            </Group>
        </Group>
    }
}

/// Background half: absolute, out of flow — the cards row keeps its
/// centered in-flow layout. Left half mid-light, right half mid-dark
/// (vertical split), both near the blend modes' pivot so the wash
/// reads in every mode against both surrounds in one frame.
///
/// Overlay runs paint NO backdrop: the overlay driver sets
/// `RIODE_COMPOSITOR_GLASS=1`, the desktop shows through, and the
/// compositor plugin renders the glass against it — a painted split
/// would cover the desktop and defeat the plugin run. The halves stay
/// for the windowed demo, where the split IS the reference backdrop
/// (near-white/near-black made Overlay invisible by construction).
fn bg_half(fill: u32, left: bool) -> Element {
    let backdrop = if std::env::var_os("RIODE_COMPOSITOR_GLASS").is_some() {
        Color::TRANSPARENT
    } else {
        Color::hex(fill)
    };
    group()
        .width(Size::Percent(0.5))
        .height(Size::Percent(1.0))
        .absolute()
        .anchor(if left { Anchor::TopLeft } else { Anchor::TopRight })
        .fill(backdrop)
        .into_element()
}
