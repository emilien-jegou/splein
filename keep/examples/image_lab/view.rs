// Single responsibility: Mipmapped image + SVG demo for wgpu Bundle 4.
//
// Three rows, each isolating one property the human judges by eye
// (F7 cycles engines on the same view for A/B):
//   1. Minification: one 256x256 high-frequency procedural source drawn at
//      128/64/32/16 px — the wgpu engine uploads a full CPU box-filter mip
//      chain, so the small sizes resolve smoothly; an engine sampling only
//      level 0 shows moire/shimmer on the rings and checker.
//   2. Premultiplied alpha: a soft radial blob (premultiplied bytes, the
//      ImageSource convention shared with tiny-skia/vello) over light and
//      dark cards — a wrong alpha convention shows fringing or dark halos.
//   3. SVG: one inline icon rasterized by resvg at device size, drawn at
//      64/32/16 px.
//
// The page is deliberately static: after the first frame every frame is a
// chunk-cache replay of image ops (Arc-held bind groups), so a vanishing
// image on frame 2+ would expose the replay path, not the draw path.

use riode::internal::*;
use riode::prelude::*;
use riode::scene::ImageSource;
use std::sync::OnceLock;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Image Lab";
pub const HOT_WIDTH: u32 = 960;
pub const HOT_HEIGHT: u32 = 720;
pub const HOT_BG: u32 = 0xE8E6E1;

const LABEL: u32 = 0x3A3F4A;
const CARD_LIGHT: u32 = 0xFFFFFF;
const CARD_DARK: u32 = 0x2A2E36;

const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
  <rect x="4" y="4" width="40" height="40" rx="10" fill="#4F7CFF"/>
  <circle cx="24" cy="24" r="11" fill="#FFFFFF"/>
  <circle cx="24" cy="24" r="5" fill="#4F7CFF"/>
</svg>"##;

/// 256x256 opaque high-frequency pattern (rings + checker + hatch): the
/// classic minification stress source. Built once; premultiplied is moot
/// at alpha 255.
fn shimmer_source() -> &'static ImageSource {
    static IMG: OnceLock<ImageSource> = OnceLock::new();
    IMG.get_or_init(|| {
        let n = 256u32;
        let mut px = Vec::with_capacity((n * n * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let fx = x as f32 / n as f32 - 0.5;
                let fy = y as f32 / n as f32 - 0.5;
                let r = (fx * fx + fy * fy).sqrt();
                let rings = ((r * 80.0).sin() * 0.5 + 0.5) * 255.0;
                let check = if (x / 4 + y / 4) % 2 == 0 { 255.0 } else { 0.0 };
                let hatch = if (x + y) % 6 == 0 { 255.0 } else { 40.0 };
                let v = (rings * 0.5 + check * 0.25 + hatch * 0.25) as u8;
                px.extend_from_slice(&[v, v.saturating_add(30), 255u8.saturating_sub(v / 2), 255]);
            }
        }
        ImageSource::from_rgba8(n, n, px)
    })
}

/// 128x128 soft radial blob, premultiplied orange (the shared ImageSource
/// byte convention). Edges fade to fully transparent black.
fn blob_source() -> &'static ImageSource {
    static IMG: OnceLock<ImageSource> = OnceLock::new();
    IMG.get_or_init(|| {
        let n = 128u32;
        let mut px = Vec::with_capacity((n * n * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let fx = x as f32 / n as f32 - 0.5;
                let fy = y as f32 / n as f32 - 0.5;
                let d = (fx * fx + fy * fy).sqrt() * 2.0;
                let a = (1.0 - d).clamp(0.0, 1.0);
                px.extend_from_slice(&[
                    (255.0 * a) as u8,
                    (140.0 * a) as u8,
                    (40.0 * a) as u8,
                    (255.0 * a) as u8,
                ]);
            }
        }
        ImageSource::from_rgba8(n, n, px)
    })
}

#[component]
fn Row(label: &'static str, kids: Vec<Element>) -> impl IntoElement {
    group()
        .direction("vertical")
        .gap(8.0)
        .children((
            text(label).size(14.0).weight(500).color(LABEL),
            group()
                .direction("horizontal")
                .gap(24.0)
                .alignment("center")
                .children(kids),
        ))
}

fn blob_card(fill: u32) -> Element {
    view! {
        <Group
            width={140.0}
            height={140.0}
            radius={10.0}
            fill={fill}
            alignment="center"
            distribution="center"
        >
            {image(blob_source().clone()).size(112.0, 112.0)}
        </Group>
    }
    .into_element()
}

#[component]
pub(crate) fn Page() -> impl IntoElement {
    let src = shimmer_source();
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={HOT_BG}
            gap={40.0}
            margin="40.0 48.0"
            alignment="start"
        >
            {Row(
                "Minification (256px source -> 128 / 64 / 32 / 16)",
                vec![
                    image(src.clone()).size(128.0, 128.0).into_element(),
                    image(src.clone()).size(64.0, 64.0).into_element(),
                    image(src.clone()).size(32.0, 32.0).into_element(),
                    image(src.clone()).size(16.0, 16.0).into_element(),
                ],
            )}
            {Row(
                "Premultiplied alpha over light / dark",
                vec![blob_card(CARD_LIGHT), blob_card(CARD_DARK)],
            )}
            {Row(
                "SVG (resvg raster at device size) 64 / 32 / 16",
                vec![
                    svg(VectorGraphic::from_str(ICON_SVG).unwrap())
                        .size(64.0, 64.0)
                        .into_element(),
                    svg(VectorGraphic::from_str(ICON_SVG).unwrap())
                        .size(32.0, 32.0)
                        .into_element(),
                    svg(VectorGraphic::from_str(ICON_SVG).unwrap())
                        .size(16.0, 16.0)
                        .into_element(),
                ],
            )}
        </Group>
    }
}
