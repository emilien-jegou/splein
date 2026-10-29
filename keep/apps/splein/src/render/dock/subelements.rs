// Paints secondary dock chrome including grip handles, dividers, actions, and popover carets.

use crate::render::icons::get_icons;
use crate::ui::dock::geometry::Box2D;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform,
};

pub fn draw_grip(grip_box: &Box2D, pix: &mut PixmapMut) {
    let icons = get_icons();
    let c = grip_box.center();
    let s = 28.0 / 21.0;
    let ts = Transform::from_translate(c.x - 10.5 * s, c.y - 10.5 * s).pre_scale(s, s);
    let mut p = Paint::default();
    p.set_color_rgba8(0x62, 0x62, 0x62, 255);
    p.anti_alias = true;
    pix.stroke_path(
        &icons.grip,
        &p,
        &Stroke {
            width: 0.9,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        },
        ts,
        None,
    );
}

pub fn draw_divider(divider_box: &Box2D, pix: &mut PixmapMut) {
    let mut pb = PathBuilder::new();
    let x = divider_box.x + 4.5;
    pb.move_to(x, divider_box.y + 6.0);
    pb.line_to(x, divider_box.y + divider_box.h - 6.0);
    if let Some(path) = pb.finish() {
        let mut p = Paint::default();
        p.set_color_rgba8(255, 255, 255, 20);
        pix.stroke_path(
            &path,
            &p,
            &Stroke {
                width: 0.5,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

pub fn draw_actions(actions: &[Box2D; 2], pix: &mut PixmapMut) {
    let icons = get_icons();
    let list = [
        (&icons.trash, Color::from_rgba8(0xFF, 0x58, 0x47, 255)),
        (&icons.more, Color::from_rgba8(0x62, 0x62, 0x62, 255)),
    ];
    for (i, box2d) in actions.iter().enumerate() {
        let c = box2d.center();
        let s = 28.0 / 21.0;
        let ts = Transform::from_translate(c.x - 10.5 * s, c.y - 10.5 * s).pre_scale(s, s);
        let mut p = Paint::default();
        p.set_color(list[i].1);
        p.anti_alias = true;
        pix.stroke_path(
            list[i].0,
            &p,
            &Stroke {
                width: 0.9,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            },
            ts,
            None,
        );
    }
}

pub fn draw_chevron(x: f32, y: f32, flip: bool, pix: &mut PixmapMut) {
    let icons = get_icons();
    let ts = if flip {
        Transform::from_translate(x + 3.0, y + 3.0)
            .pre_rotate(180.0)
            .pre_translate(-3.0, -3.0)
            .pre_scale(6.0 / 4.5, 6.0 / 4.5)
    } else {
        Transform::from_translate(x, y).pre_scale(6.0 / 4.5, 6.0 / 4.5)
    };
    let mut p = Paint::default();
    p.set_color(Color::WHITE);
    p.anti_alias = true;
    pix.fill_path(&icons.chevron, &p, FillRule::Winding, ts, None);
}

pub fn draw_caret(cx: f32, top_y: f32, pix: &mut PixmapMut) {
    let size = 13.727;
    let half = size * 0.5;
    let ts = Transform::from_translate(cx, top_y - 2.0)
        .pre_rotate(45.0)
        .pre_translate(-half, -half);
    let mut pb = PathBuilder::new();
    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, size, size) {
        pb.push_rect(r);
        if let Some(p) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, 255);
            paint.anti_alias = true;
            pix.fill_path(&p, &paint, FillRule::Winding, ts, None);
        }
    }
}
