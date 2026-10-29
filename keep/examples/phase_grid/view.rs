// Single responsibility: Static Y-eighth phase grid for subpixel text triage.
//
// Six sizes (10/12/14/16/24/36px), each rendered at all eight Y-eighth
// device offsets (k/8 px, k = 0..7) in stacked rows: identical glyphs,
// identical X, only the fractional Y remainder varies. Small sizes show
// the whole alphabet + digits; large sizes show horizontal-feature-rich
// letters (t e l H o n T E 0 5, including the reported "t" crossbar).
// Static (no motion): F8 freeze + `s` capture, zoom the captures to compare
// phases. Human judges; no test asserts.

use super::motion_ui::*;
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this file composes repr-builders directly

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Phase grid repro";
pub const HOT_WIDTH: u32 = 1040;
pub const HOT_HEIGHT: u32 = 1380;
pub const HOT_BG: u32 = PAGE;

/// Phase labels, one per Y-eighth row.
const PHASES: [&str; 8] = ["0/8", "1/8", "2/8", "3/8", "4/8", "5/8", "6/8", "7/8"];
/// Full alphabet + digits for small sizes.
const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz 0123456789";
/// Horizontal-feature-rich sample for large sizes.
const LETTERS: &str = "t e l H o n T E 0 5";

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Shell
            eyebrow_label="TEXT · PHASE GRID"
            title="One size per section, all eight Y-eighths."
                copy="Static rows isolate Y placement: same glyphs, same X, only the fractional Y remainder (k/8 px) varies down each section. Small sizes carry the full alphabet, large sizes horizontal-rich letters. Freeze (F8) and capture (s), then zoom: rest-frame dirt that changes with phase is placement residue; dirt identical on all eight is coverage or composite. Cycle engines (F7)."
        >
            <Content>
                <Group direction="vertical" width="fill" height="fill" gap={20.0}>
                    <SizeSection size={10.0} sample={ALPHABET} height={20.0} label="10PX · ALPHABET" />
                    <SizeSection size={12.0} sample={ALPHABET} height={22.0} label="12PX · ALPHABET" />
                    <SizeSection size={14.0} sample={ALPHABET} height={24.0} label="14PX · ALPHABET" />
                    <SizeSection size={16.0} sample={ALPHABET} height={26.0} label="16PX · ALPHABET" />
                    <SizeSection size={24.0} sample={LETTERS} height={40.0} label="24PX · LETTERS" />
                    <SizeSection size={36.0} sample={LETTERS} height={56.0} label="36PX · LETTERS" />
                </Group>
            </Content>
        </Shell>
    }
}

/// One size section: header plus eight phase rows at Y offsets k/8 px.
#[component]
fn SizeSection(size: f32, sample: &'static str, height: f32, label: &'static str) -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" gap={2.0}>
            <Group height={20.0} alignment="center">
                <Eyebrow label={label} />
            </Group>
            <PhaseRow size={size} sample={sample} height={height} phase={0} label={PHASES[0]} />
            <PhaseRow size={size} sample={sample} height={height} phase={1} label={PHASES[1]} />
            <PhaseRow size={size} sample={sample} height={height} phase={2} label={PHASES[2]} />
            <PhaseRow size={size} sample={sample} height={height} phase={3} label={PHASES[3]} />
            <PhaseRow size={size} sample={sample} height={height} phase={4} label={PHASES[4]} />
            <PhaseRow size={size} sample={sample} height={height} phase={5} label={PHASES[5]} />
            <PhaseRow size={size} sample={sample} height={height} phase={6} label={PHASES[6]} />
            <PhaseRow size={size} sample={sample} height={height} phase={7} label={PHASES[7]} />
        </Group>
    }
}

/// One phase row: the sample text shifted down exactly phase/8 device px.
#[component]
fn PhaseRow(size: f32, sample: &'static str, height: f32, phase: i32, label: &'static str) -> impl IntoElement {
    let dy = phase as f32 / 8.0;
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={height}
            alignment="center"
            gap={12.0}
        >
            <Group width={36.0} height="fill" alignment="center">
                <Eyebrow label={label} />
            </Group>
            {group().children([phase_text(sample, size)]).y(dy)}
        </Group>
    }
}

/// Shared sample copy at one size, default weight (matches dirt reports).
fn phase_text(copy: &'static str, size: f32) -> TextDef {
    text(copy).size(size).color(INK)
}
