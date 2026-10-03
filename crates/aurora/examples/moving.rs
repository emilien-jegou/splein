// Single responsibility: Showcase DVD bouncing overlay with CLI engine selection.

#[path = "common/mod.rs"]
mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use aurora::app::AppExtension;
use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use aurora::scene::VectorGraphic;
use common::ExampleCli;
use winit::event::WindowEvent;

const DVD_SVG_RAW: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<svg xmlns="http://www.w3.org/2000/svg" version="1.0" width="140" height="66.2" viewBox="-2.24251512 -2.24251512 167.56476024 79.23553424">
  <g transform="translate(-944.17443,-1086.4155)" fill="currentColor">
    <path d="M 1096.2843,1155.4734 L 1094.4417,1155.4734 L 1093.6887,1161.166 L 1092.7674,1161.166 L 1093.604,1155.4734 L 1091.7625,1155.4734 L 1091.8461,1154.7181 L 1096.3667,1154.7181 L 1096.2843,1155.4734 L 1096.2843,1155.4734 z M 1102.9817,1161.166 L 1102.0604,1161.166 L 1102.0604,1156.2253 L 1102.0604,1156.2253 L 1099.4648,1161.166 L 1098.3764,1156.2253 L 1098.3764,1156.2253 L 1096.7868,1161.166 L 1095.9502,1161.166 L 1098.0411,1154.7181 L 1098.7964,1154.7181 L 1099.8013,1159.0739 L 1102.0604,1154.7181 L 1102.9817,1154.7181 L 1102.9817,1161.166 L 1102.9817,1161.166 z M 1080.8915,1086.484 L 1047.5414,1086.484 C 1047.5414,1086.484 1038.7032,1096.9118 1037.0451,1098.9145 C 1028.2777,1109.477 1026.6904,1112.3093 1026.4119,1113.0693 C 1026.4815,1112.3093 1026.1346,1109.477 1022.4064,1098.7741 C 1021.3726,1095.8739 1018.1945,1086.484 1018.1945,1086.484 L 1006.8024,1086.484 L 1006.8024,1086.4155 L 979.04393,1086.484 L 958.19232,1086.484 L 956.05093,1095.3233 L 971.86269,1095.3912 L 975.52119,1095.3912 C 985.67172,1095.3912 991.88874,1099.4651 990.16101,1106.6475 C 988.29637,1114.5186 979.3874,1117.9711 969.9992,1117.9711 L 966.47646,1117.9711 L 971.03421,1098.6384 L 955.29267,1098.6384 L 948.59352,1126.8784 L 970.96576,1126.8784 C 987.74289,1126.8784 1003.7647,1118.0396 1006.5274,1106.6475 C 1007.0786,1104.5746 1007.0101,1099.3955 1005.6966,1096.2899 C 1005.6966,1096.1512 1005.6281,1096.0822 1005.5597,1095.806 C 1005.4889,1095.7376 1005.4205,1095.2531 1005.6966,1095.1858 C 1005.8358,1095.1162 1006.1132,1095.3912 1006.1816,1095.4608 C 1006.3185,1095.806 1006.3882,1096.0822 1006.3882,1096.0822 L 1020.6114,1136.2689 L 1056.8623,1095.3912 L 1072.1913,1095.3912 L 1075.851,1095.3912 C 1086.0015,1095.3912 1092.2174,1099.4651 1090.5581,1106.6475 C 1088.625,1114.5186 1079.7172,1117.9711 1070.3279,1117.9711 L 1066.8063,1117.9711 L 1071.3629,1098.6384 L 1055.6196,1098.6384 L 1048.9233,1126.8784 L 1071.2944,1126.8784 C 1088.0739,1126.8784 1104.1629,1118.0396 1106.8549,1106.6475 C 1109.6165,1095.3233 1097.7394,1086.484 1080.8915,1086.484 L 1080.8915,1086.484 z M 1021.1649,1136.407 C 978.63087,1136.407 944.17443,1141.3105 944.17443,1147.3163 C 944.17443,1153.393 978.63087,1158.2257 1021.1649,1158.2257 C 1063.6978,1158.2257 1098.2221,1153.393 1098.2221,1147.3163 C 1098.2221,1141.3105 1063.6978,1136.407 1021.1649,1136.407 L 1021.1649,1136.407 z M 1018.4022,1151.1825 C 1008.6647,1151.1825 1000.7942,1149.5268 1000.7942,1147.524 C 1000.7942,1145.5236 1008.6647,1143.9328 1018.4022,1143.9328 C 1028.1384,1143.9328 1036.0101,1145.5236 1036.0101,1147.524 C 1036.0101,1149.5268 1028.1384,1151.1825 1018.4022,1151.1825 L 1018.4022,1151.1825 z" style="fill-rule:evenodd" />
  </g>
</svg>"#;

const LOGO_W: f32 = 140.0;
const LOGO_H: f32 = 66.2;

const BOUNCE_HEX: &[u32] = &[0xEC4899, 0x38BDF8, 0xFACC15, 0x4ADE80, 0xA855F7, 0xFB923C];

pub struct DvdPhysicsDriver {
    pos_x: f32,
    pos_y: f32,
    vel_x: f32,
    vel_y: f32,
    win_w: f32,
    win_h: f32,
    color_idx: usize,
    last_time: Instant,
    pos_signal: Signal<(f32, f32)>,
    color_signal: Signal<Color>,
}

impl DvdPhysicsDriver {
    pub fn new(
        w: f32,
        h: f32,
        pos_signal: Signal<(f32, f32)>,
        color_signal: Signal<Color>,
    ) -> Self {
        Self {
            pos_x: 60.0,
            pos_y: 60.0,
            vel_x: 220.0,
            vel_y: 160.0,
            win_w: w,
            win_h: h,
            color_idx: 0,
            last_time: Instant::now(),
            pos_signal,
            color_signal,
        }
    }

    fn next_color(&mut self) -> Color {
        self.color_idx = (self.color_idx + 1) % BOUNCE_HEX.len();
        Color::hex(BOUNCE_HEX[self.color_idx])
    }
}

impl AppExtension for DvdPhysicsDriver {
    fn on_event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::Resized(size) = event {
            self.win_w = size.width as f32;
            self.win_h = size.height as f32;
            self.pos_x = self.pos_x.min((self.win_w - LOGO_W).max(0.0));
            self.pos_y = self.pos_y.min((self.win_h - LOGO_H).max(0.0));
            return true;
        }
        false
    }

    fn on_frame(&mut self, _diagnostics: &aurora::runtime::FrameDiagnostics) {
        let now = Instant::now();
        let dt = (now - self.last_time).as_secs_f32().min(0.05);
        self.last_time = now;

        self.pos_x += self.vel_x * dt;
        self.pos_y += self.vel_y * dt;

        let max_x = (self.win_w - LOGO_W).max(0.0);
        let max_y = (self.win_h - LOGO_H).max(0.0);
        let mut hit = false;

        if self.pos_x >= max_x {
            self.pos_x = max_x;
            self.vel_x = -self.vel_x.abs();
            hit = true;
        } else if self.pos_x <= 0.0 {
            self.pos_x = 0.0;
            self.vel_x = self.vel_x.abs();
            hit = true;
        }

        if self.pos_y >= max_y {
            self.pos_y = max_y;
            self.vel_y = -self.vel_y.abs();
            hit = true;
        } else if self.pos_y <= 0.0 {
            self.pos_y = 0.0;
            self.vel_y = self.vel_y.abs();
            hit = true;
        }

        if hit {
            let next_c = self.next_color();
            self.color_signal.set(next_c);
        }

        self.pos_signal.set((self.pos_x, self.pos_y));
    }
}

fn main() {
    let cli = ExampleCli::parse(800, 600);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let pos_signal = Signal::new(Rc::clone(&runtime), (60.0f32, 60.0f32));
    let color_signal = Signal::new(Rc::clone(&runtime), Color::hex(BOUNCE_HEX[0]));

    let driver = DvdPhysicsDriver::new(
        cli.width as f32,
        cli.height as f32,
        pos_signal.clone(),
        color_signal.clone(),
    );

    cli.app_config("Aurora — DVD Bounce Screensaver")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .runtime(runtime)
        .extension(driver)
        .run(move || build_ui(pos_signal.clone(), color_signal.clone()));
}

fn build_ui(pos_sig: Signal<(f32, f32)>, color_sig: Signal<Color>) -> impl IntoElement {
    let logo_graphic = VectorGraphic::from_str(DVD_SVG_RAW).expect("Valid SVG");

    group().direction(Direction::Vertical)
        .width(Size::fill()).height(Size::fill())
        .fill(Color::hex(0x090D16))
        .children((
            group().direction(Direction::Vertical)
                .width(Size::fill()).height(Size::fill())
                .gap(20.0)
                .margin(Margin::all(24.0))
                .children([
                    group().direction(Direction::Horizontal)
                        .width(Size::fill())
                        .height(Size::fit())
                        .distribution(Distribution::SpaceBetween)
                        .alignment(Alignment::Center)
                        .children((
                            group().direction(Direction::Vertical).gap(4.0).children([
                                text("Aurora Rendering Showcase")
                                    .size(20.0)
                                    .weight(700)
                                    .color(Color::WHITE),
                                text("DVD Vector Overlay bouncing over nested Flex containers")
                                    .size(13.0)
                                    .color(Color::hex(0x64748B)),
                            ]),
                            group()
                                .width(Size::fit()).height(Size::fit())
                                .radius(Radius::scalar(999.0))
                                .fill(Color::hex(0x1E293B))
                                .stroke(Stroke::inside(1.0, Color::hex(0x334155)))
                                .margin(Margin::sides(6.0, 12.0, 6.0, 12.0))
                                .children([
                                    text("Press [F12] Damage | [F11] Boundaries")
                                        .size(12.0)
                                        .color(Color::hex(0x38BDF8)),
                                ]),
                        )),
                    group().direction(Direction::Horizontal)
                        .width(Size::fill())
                        .height(Size::fill())
                        .gap(16.0)
                        .children((
                            group().direction(Direction::Vertical)
                                .width(Size::Percent(0.6))
                                .height(Size::fill())
                                .gap(16.0)
                                .children([
                                    group()
                                        .width(Size::fill())
                                        .height(180.0)
                                        .radius(Radius::scalar(16.0))
                                        .fill(Color::hex(0x131C2E))
                                        .stroke(Stroke::inside(1.0, Color::hex(0x1E293B)))
                                        .margin(Margin::all(16.0))
                                        .children([
                                            group().direction(Direction::Vertical).gap(12.0).children([
                                                text("Subtree Isolation Matrix")
                                                    .size(15.0)
                                                    .weight(600)
                                                    .color(Color::WHITE),
                                                text("The DVD logo hovers in an overlay z-plane. Moving affine translations incur O(1) dirty costs, strictly bounding spatial damage to its current and previous scanlines.")
                                                    .size(13.0)
                                                    .line_height(18.0)
                                                    .color(Color::hex(0x94A3B8)),
                                            ]),
                                        ]),
                                    group()
                                        .width(Size::fill()).height(Size::fill())
                                        .radius(Radius::scalar(16.0))
                                        .fill(Color::hex(0x131C2E))
                                        .stroke(Stroke::inside(1.0, Color::hex(0x1E293B)))
                                        .margin(Margin::all(16.0))
                                        .children([
                                            group().direction(Direction::Vertical).gap(12.0).children((
                                                text("Nested Geometry Containers")
                                                    .size(15.0)
                                                    .weight(600)
                                                    .color(Color::WHITE),
                                                group().direction(Direction::Horizontal).gap(8.0).children([
                                                    group().width(80.0).height(60.0).radius(Radius::scalar(8.0)).fill(Color::hex(0x1E293B)),
                                                    group().width(80.0).height(60.0).radius(Radius::scalar(8.0)).fill(Color::hex(0x1E293B)),
                                                    group().width(80.0).height(60.0).radius(Radius::scalar(8.0)).fill(Color::hex(0x1E293B)),
                                                ]),
                                            )),
                                        ]),
                                ]),
                            group().direction(Direction::Vertical)
                                .width(Size::Percent(0.4))
                                .height(Size::fill())
                                .gap(16.0)
                                .children([
                                    group()
                                        .width(Size::fill()).height(Size::fill())
                                        .radius(Radius::scalar(16.0))
                                        .fill(Color::hex(0x0F172A))
                                        .stroke(Stroke::inside(1.0, Color::hex(0x1E293B)))
                                        .margin(Margin::all(16.0))
                                        .children([
                                            group().direction(Direction::Vertical).gap(10.0).children([
                                                text("Live Vector Telemetry")
                                                    .size(15.0)
                                                    .weight(600)
                                                    .color(Color::WHITE),
                                                text("SVG ViewBox: 167.5 x 79.2")
                                                    .size(12.0)
                                                    .color(Color::hex(0x64748B)),
                                                text("Render Mode: Vector Anti-aliased TinySkia / Vello")
                                                    .size(12.0)
                                                    .color(Color::hex(0x64748B)),
                                            ]),
                                        ]),
                                ]),
                        )),
                ]),
            group()
                .width(LOGO_W).height(LOGO_H)
                .overlay(true).z_index(100)
                .anchor(Anchor::TopLeft)
                .fill(color_sig)
                .transform(move || {
                    let (x, y) = pos_sig.get();
                    Transform::from_translation(x, y)
                })
                .children([
                    svg(logo_graphic),
                ]),
        ))
}
