// Renders the Paper dual-inset shadow glow and translucent fill.

use super::spectrum::sample_spectrum;
use tiny_skia::{
    Color, FillRule, LinearGradient, Paint, PathBuilder, PixmapMut, Point, Rect, Stroke, Transform,
};

pub fn draw_backdrop(x: f32, y: f32, w: f32, h: f32, track_x: f32, pix: &mut PixmapMut) {
    let Some(r) = Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(r);
    let Some(path) = pb.finish() else {
        return;
    };

    // 1. Base translucent fill (13% alpha matching Paper #57FA5821)
    let mut fill_paint = Paint::default();
    fill_paint.set_color(sample_spectrum(track_x, 0.13));
    fill_paint.anti_alias = true;
    pix.fill_path(
        &path,
        &fill_paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // 2. Dual Inset Shadow (Top and Bottom glow gradients matching Paper #57FA582E)
    let glow = sample_spectrum(track_x, 0.18);
    let transparent_glow =
        Color::from_rgba(glow.red(), glow.green(), glow.blue(), 0.0).unwrap_or(Color::TRANSPARENT);

    if let Some(shader) = LinearGradient::new(
        Point::from_xy(x, y),
        Point::from_xy(x, y + 4.0),
        vec![
            tiny_skia::GradientStop::new(0.0, glow),
            tiny_skia::GradientStop::new(1.0, transparent_glow),
        ],
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    ) {
        let mut top_paint = Paint::default();
        top_paint.shader = shader;
        pix.fill_path(
            &path,
            &top_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // 3. Perimeter hairline
    let mut stroke_paint = Paint::default();
    stroke_paint.set_color(glow);
    pix.stroke_path(
        &path,
        &stroke_paint,
        &Stroke {
            width: 0.5,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}
