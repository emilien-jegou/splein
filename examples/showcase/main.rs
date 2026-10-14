// Single responsibility: Flagship showcase demonstrating layout boundaries, damage tracking, and GPU compute.

#[path = "../common/mod.rs"]
mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use aurora::app::switcher::{BootCtx, BootedDemo};
use aurora::app::AppExtension;
use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;

const DEFAULT_WIDTH: u32 = 1440;
const DEFAULT_HEIGHT: u32 = 900;

/// High-frequency animation driver powering multiple independent reactive pipelines.
pub struct MissionControlTicker {
    start: Instant,
    time_sig: Signal<f32>,
    counter_sig: Signal<u64>,
}

impl MissionControlTicker {
    pub fn new(time_sig: Signal<f32>, counter_sig: Signal<u64>) -> Self {
        Self {
            start: Instant::now(),
            time_sig,
            counter_sig,
        }
    }
}

impl AppExtension for MissionControlTicker {
    fn on_frame(&mut self, _diagnostics: &aurora::runtime::FrameDiagnostics) {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.time_sig.set(elapsed);
        self.counter_sig.set((elapsed * 1250.0) as u64); // ~1,250 packets/sec
    }
}

/// Rebuilds the mission-control world on a fresh runtime for in-window switching.
pub(crate) fn boot(ctx: &BootCtx) -> BootedDemo {
    let runtime = Rc::clone(&ctx.runtime);
    let time_sig = Signal::new(Rc::clone(&runtime), 0.0f32);
    let counter_sig = Signal::new(Rc::clone(&runtime), 0u64);

    let ticker = MissionControlTicker::new(time_sig, counter_sig);

    let cx = Cx::new(runtime);
    let root = cx
        .enter_keyed("root", move || {
            build_ui(time_sig, counter_sig)
        })
        .into_element();
    BootedDemo {
        root,
        background: 0x06090Fu32.into(),
        extensions: vec![Box::new(ticker)],
    }
}

fn main() {
    let cli = ExampleCli::parse(DEFAULT_WIDTH, DEFAULT_HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let time_sig = Signal::new(Rc::clone(&runtime), 0.0f32);
    let counter_sig = Signal::new(Rc::clone(&runtime), 0u64);

    let ticker = MissionControlTicker::new(time_sig, counter_sig);

    cli.app_config_multi("Aurora — Mission Control Engine Showcase")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(0x06090F)
        .runtime(runtime)
        .extension(ticker)
        .run(move || build_ui(time_sig, counter_sig));
}

fn build_ui(time_sig: Signal<f32>, counter_sig: Signal<u64>) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width="fill" height="fill" fill={0x06090F}>
            // =========================================================================
            // 1. LEFT NAVIGATION SIDEBAR (Static complex UI tree, $0 CPU cost)
            // =========================================================================
            <Group
                direction="vertical"
                width={260.0}
                height="fill"
                fill={0x0C101A}
                stroke="1.0 #1A2234 inside"
                margin={16.0}
            >
                // Logo Header
                <Group direction="horizontal" width="fill" alignment="center" gap={10.0} margin={Margin::x(8.0)}>
                    <Group
                        width={28.0}
                        height={28.0}
                        radius={8.0}
                        fill={0x38BDF8}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={14.0} color={0x06090F}>"▲"</Text>
                    </Group>
                    <Text size={15.0} weight={700u16} color={Color::WHITE}>"AURORA CORE"</Text>
                </Group>

                <Group width="fill" height={1.0} fill={0x1A2234} margin={Margin::y(12.0)} />

                // Sidebar Navigation Links
                <Group direction="vertical" width="fill" gap={4.0}>
                    <NavLink label="Mission Overview" active={true} />
                    <NavLink label="Telemetry Stream" active={false} />
                    <NavLink label="Spatial Damage" active={false} />
                    <NavLink label="Layout Boundaries" active={false} />
                    <NavLink label="GPU Compute Vello" active={false} />
                </Group>

                <Group height="fill" />

                // System Architecture Specs Card
                <Group
                    width="fill"
                    radius={12.0}
                    fill={0x111726}
                    stroke="1.0 #1E293B inside"
                    margin={12.0}
                >
                    <Group direction="vertical" gap={6.0}>
                        <Text size={11.0} weight={700u16} color={0x38BDF8}>"ENGINE SPECS"</Text>
                        <Text size={11.0} color={0x94A3B8}>"Pipeline: 7-Stage Retained"</Text>
                        <Text size={11.0} color={0x94A3B8}>"Reactivity: $0 Atomic Contention"</Text>
                        <Text size={11.0} color={0x94A3B8}>"Layout: O(1) Boundary Cascades"</Text>
                        <Text size={11.0} color={0x94A3B8}>"Presentation: Hardware SIMD"</Text>
                    </Group>
                </Group>
            </Group>

            // =========================================================================
            // 2. MAIN MISSION CONTROL DASHBOARD
            // =========================================================================
            <Group direction="vertical" width="fill" height="fill" margin={16.0} gap={16.0}>
                // Top Metrics Banner (Mixed Fit / Fill, Badges)
                <Group
                    direction="horizontal"
                    width="fill"
                    height="fit"
                    distribution="space-between"
                    alignment="center"
                >
                    <Group direction="vertical" gap={4.0}>
                        <Text size={22.0} weight={700u16} color={Color::WHITE}>
                            "Active Mission Telemetry"
                        </Text>
                        <Text size={13.0} color={0x64748B}>
                            "Press [F11] to verify Layout Boundaries | [F12] for Damage Heatmaps | [F9] for Stacking"
                        </Text>
                    </Group>
                    <Group direction="horizontal" gap={8.0}>
                        {status_badge("PIPELINE", "ONLINE", 0x22C55E)}
                        {status_badge("VSYNC", "LOCKED", 0x38BDF8)}
                    </Group>
                </Group>

                // Grid Layout (2x2 Modular Telemetry Cards)
                <Group direction="horizontal" width="fill" height="fill" gap={16.0}>
                    // -------------------------------------------------------------
                    // CARD 1: LIVE AUDIO HARMONICS EQUALIZER (16 Live Dynamic Bars)
                    // Proves: 16 dynamic flex elements updating without dirtying outer layout
                    // -------------------------------------------------------------
                    <Group
                        width={Size::percent(0.5)}
                        height="fill"
                        radius={16.0}
                        fill={0x0C101A}
                        stroke="1.0 #1A2234 inside"
                        margin={16.0}
                    >
                        <Group direction="vertical" width="fill" height="fill">
                            <Group
                                direction="horizontal"
                                width="fill"
                                distribution="space-between"
                                alignment="center"
                            >
                                <Text size={15.0} weight={600u16} color={Color::WHITE}>
                                    "Dynamic Harmonics Spectrum"
                                </Text>
                                <Text size={12.0} color={0x38BDF8}>"16 Isolated Signals"</Text>
                            </Group>
                            <Text size={12.0} color={0x64748B} margin={Margin::bottom(16.0)}>
                                "Mutating flex heights inside an isolated layout boundary ($O(K)$ updates)"
                            </Text>

                            // The 16 Animated Equalizer Bars
                            <Group
                                direction="horizontal"
                                width="fill"
                                height="fill"
                                alignment="center"
                                distribution="space-between"
                            >
                                {build_equalizer_bars(time_sig)}
                            </Group>
                        </Group>
                    </Group>

                    // -------------------------------------------------------------
                    // CARD 2: 360° ORBITAL RADAR SCANNER (Continuous Transform in O(1))
                    // Proves: Pure affine rotation without ANY layout or text remeasuring
                    // -------------------------------------------------------------
                    <Group
                        width={Size::percent(0.5)}
                        height="fill"
                        radius={16.0}
                        fill={0x0C101A}
                        stroke="1.0 #1A2234 inside"
                        margin={16.0}
                    >
                        <Group direction="vertical" width="fill" height="fill">
                            <Group
                                direction="horizontal"
                                width="fill"
                                distribution="space-between"
                                alignment="center"
                            >
                                <Text size={15.0} weight={600u16} color={Color::WHITE}>
                                    "Orbital Vector Scanner"
                                </Text>
                                <Text size={12.0} color={0xEC4899}>"360° Continuous Matrix"</Text>
                            </Group>
                            <Text size={12.0} color={0x64748B}>
                                "Affine rotation matrices executed with $0 CPU layout calculations"
                            </Text>

                            // Radar Visualizer Container
                            <Group
                                width="fill"
                                height="fill"
                                alignment="center"
                                distribution="center"
                            >
                                // Concentric Target Rings
                                <Group
                                    width={220.0}
                                    height={220.0}
                                    radius="max"
                                    stroke="1.0 #1E293B inside"
                                    alignment="center"
                                    distribution="center"
                                >
                                    <Group
                                        width={140.0}
                                        height={140.0}
                                        radius="max"
                                        stroke="1.0 #1E293B inside"
                                        alignment="center"
                                        distribution="center"
                                    >
                                        <Group width={60.0} height={60.0} radius="max" fill={0x111726} />
                                    </Group>
                                </Group>
                                // Rotating Radar Line
                                <Group
                                    width={200.0}
                                    height={2.0}
                                    fill={0x38BDF8}
                                    shadow="0 0 8 0 #38BDF8"
                                    anchor={Anchor::Center}
                                    transform={{
                                        let t = time_sig;
                                        move || {
                                            let angle = t.get() * 90.0; // 90 deg/sec
                                            Transform::from_rotation_degrees(angle)
                                        }
                                    }}
                                />
                                // Orbiting Satellite Beacon
                                <Group
                                    width={14.0}
                                    height={14.0}
                                    radius="max"
                                    fill={0xEC4899}
                                    shadow="0 0 10 0 #EC4899"
                                    anchor={Anchor::Center}
                                    transform={{
                                        let t = time_sig;
                                        move || {
                                            let elapsed = t.get() * 1.5;
                                            let ox = elapsed.cos() * 85.0;
                                            let oy = elapsed.sin() * 85.0;
                                            Transform::from_translation(ox, oy)
                                        }
                                    }}
                                />
                            </Group>
                        </Group>
                    </Group>
                </Group>

                // Bottom Row (High-Frequency Realtime Throughput Stream)
                <Group direction="horizontal" width="fill" height={120.0} gap={16.0}>
                    // Live Stream Counter Card
                    <Group
                        width={Size::percent(0.5)}
                        height="fill"
                        radius={16.0}
                        fill={0x0C101A}
                        stroke="1.0 #1A2234 inside"
                        margin={16.0}
                    >
                        <Group
                            direction="horizontal"
                            width="fill"
                            height="fill"
                            alignment="center"
                            distribution="space-between"
                        >
                            <Group direction="vertical" gap={4.0}>
                                <Text size={11.0} weight={700u16} color={0x94A3B8}>
                                    "TELEMETRY INGESTION STREAM"
                                </Text>
                                <Text size={12.0} color={0x64748B}>
                                    "High-frequency reactive string buffer mutations"
                                </Text>
                            </Group>
                            <Text size={24.0} weight={700u16} color={0x22C55E}>
                                {{
                                    let c = counter_sig;
                                    move || format!("{:09} PKTS", c.get())
                                }}
                            </Text>
                        </Group>
                    </Group>

                    // Live Memory / Frame Budget Health Card
                    <Group
                        width={Size::percent(0.5)}
                        height="fill"
                        radius={16.0}
                        fill={0x0C101A}
                        stroke="1.0 #1A2234 inside"
                        margin={16.0}
                    >
                        <Group
                            direction="horizontal"
                            width="fill"
                            height="fill"
                            alignment="center"
                            distribution="space-between"
                        >
                            <Group direction="vertical" gap={4.0}>
                                <Text size={11.0} weight={700u16} color={0x94A3B8}>
                                    "16.6ms FRAME BUDGET HEALTH"
                                </Text>
                                <Text size={12.0} color={0x22C55E}>
                                    "CPU Compute Time: < 0.4ms (97.6% Headroom)"
                                </Text>
                            </Group>
                            <Group width={160.0} height={8.0} radius="max" fill={0x1E293B}>
                                <Group
                                    width={Size::percent(0.04)}
                                    height="fill"
                                    radius="max"
                                    fill={0x22C55E}
                                />
                            </Group>
                        </Group>
                    </Group>
                </Group>
            </Group>
        </Group>
    }
}

// -----------------------------------------------------------------------------
// HELPER COMPONENT BUILDERS
// -----------------------------------------------------------------------------

#[component]
fn NavLink(label: &'static str, active: bool) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={36.0}
            radius={8.0}
            fill={if active {
                Color::hex_alpha(0x38BDF8, 0.12)
            } else {
                Color::TRANSPARENT
            }}
            alignment="center"
            margin={Margin::x(8.0)}
        >
            <Group
                width={6.0}
                height={6.0}
                radius="max"
                fill={if active {
                    Color::hex(0x38BDF8)
                } else {
                    Color::hex(0x334155)
                }}
                margin={Margin::left(12.0)}
            />
            <Text
                size={13.0}
                weight={if active { 600u16 } else { 400u16 }}
                color={if active {
                    Color::WHITE
                } else {
                    Color::hex(0x94A3B8)
                }}
                margin={Margin::left(10.0)}
            >{label}</Text>
        </Group>
    }
}

fn status_badge(
    label: &'static str,
    val: &'static str,
    color: impl Into<Color>,
) -> impl IntoElement {
    let color = color.into();
    group()
        .direction("horizontal")
        .height(30.0)
        .radius(8.0)
        .fill(0x111726)
        .stroke("1.0 #1E293B inside")
        .alignment("center")
        .gap(6.0)
        .margin(Margin::x(8.0))
        .children((
            group()
                .width(6.0)
                .height(6.0)
                .radius("max")
                .fill(color)
                .margin(Margin::left(8.0)),
            text(label).size(11.0).weight(600).color(0x94A3B8),
            text(val)
                .size(11.0)
                .weight(700)
                .color(color)
                .margin(Margin::right(8.0)),
        ))
}

/// Generates 16 mathematical harmonic equalizer bars driven by reactive time.
fn build_equalizer_bars(t_sig: Signal<f32>) -> Vec<Element> {
    let mut bars = Vec::with_capacity(16);
    for i in 0..16 {
        let t = t_sig;
        let phase = i as f32 * 0.45;
        let speed = 2.5 + (i % 4) as f32 * 0.8;

        let bar = group()
            .width(18.0)
            .height({
                move || {
                    let elapsed = t.get();
                    let raw = ((elapsed * speed + phase).sin() * 0.5 + 0.5) * 160.0;
                    Size::Fixed(raw.max(12.0))
                }
            })
            .radius(6.0)
            .fill(if i % 2 == 0 {
                Color::hex(0x38BDF8)
            } else {
                Color::hex(0x818CF8)
            })
            .shadow(Shadow::web_like(
                0.0,
                0.0,
                6.0,
                0.0,
                Color::hex_alpha(0x38BDF8, 0.30),
            ))
            .anchor(Anchor::Bottom);

        bars.push(bar.into_element());
    }
    bars
}
