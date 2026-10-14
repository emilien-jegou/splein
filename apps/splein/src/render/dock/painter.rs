// Coordinates tiny-skia drawing passes for the main dock bar and submenu popovers.

use super::animation::SlideMotion;
use super::backdrop::draw_backdrop;
use super::spectrum::active_shortcut_color;
use super::subelements::{draw_actions, draw_caret, draw_chevron, draw_divider, draw_grip};
use super::text::TextEngine;
use crate::render::icons::get_icons;
use crate::ui::dock::geometry::DockGeometry;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

pub fn render_dock(
    geom: &DockGeometry,
    motion: &SlideMotion,
    text: &mut TextEngine,
    active_i: usize,
    active_shape: usize,
    submenu_open: bool,
    pix: &mut PixmapMut,
) {
    if let Some(r) = Rect::from_xywh(
        geom.container.x,
        geom.container.y,
        geom.container.w,
        geom.container.h,
    ) {
        let mut pb = PathBuilder::new();
        pb.push_rect(r);
        if let Some(p) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, 255);
            paint.anti_alias = true;
            pix.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    draw_backdrop(
        geom.tools[0].x + motion.current_rel_x,
        geom.container.y + 3.0,
        motion.current_w,
        42.0,
        motion.current_rel_x,
        pix,
    );
    draw_grip(&geom.grip, pix);
    draw_divider(&geom.divider, pix);
    draw_actions(&geom.actions, pix);

    let icons = get_icons();
    let shape_icon = match active_shape {
        0 => &icons.shape_ray,
        1 => &icons.shape_line,
        3 => &icons.shape_circle,
        _ => &icons.shape_rect,
    };
    let tool_paths = [
        &icons.tool_q,
        &icons.tool_w,
        &icons.tool_e,
        shape_icon,
        &icons.tool_t,
        &icons.tool_y,
    ];
    let tool_colors = [
        Color::from_rgba8(0xFF, 0xA0, 0x00, 255),
        Color::from_rgba8(0x57, 0xFA, 0x58, 255),
        Color::from_rgba8(0xFA, 0xFF, 0x20, 255),
        Color::from_rgba8(0x00, 0xAE, 0xFF, 255),
        Color::from_rgba8(0x00, 0xDE, 0xB3, 255),
        Color::from_rgba8(0xFF, 0x64, 0xD4, 255),
    ];
    let shortcuts = ["q", "w", "e", "r", "t", "y"];

    for (i, b) in geom.tools.iter().enumerate() {
        let is_exp = i == 0 || i == 3;
        let icon_ox = if is_exp {
            b.x + (b.w - 34.0) * 0.5
        } else {
            b.x + (b.w - 28.0) * 0.5
        };
        let icon_oy = b.y + (b.h - 28.0) * 0.5;

        let scale = 28.0 / 21.0;
        let ts = Transform::from_translate(icon_ox, icon_oy).pre_scale(scale, scale);
        let mut p = Paint::default();
        p.set_color(tool_colors[i]);
        p.anti_alias = true;
        let s = Stroke {
            width: 0.9,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        pix.stroke_path(tool_paths[i], &p, &s, ts, None);

        if is_exp {
            let flip = i == 3 && submenu_open;
            draw_chevron(icon_ox + 28.0 + 2.0, b.y + (b.h - 6.0) * 0.5, flip, pix);
        }

        let badge_offset = if i == 5 { -2.0 } else { 0.0 };
        let sc_color = if i == active_i {
            active_shortcut_color(i)
        } else {
            Color::from_rgba8(0x62, 0x62, 0x62, 255)
        };
        text.render_text(
            shortcuts[i],
            b.x + b.w - 12.0 - badge_offset,
            b.y + b.h - 10.0,
            sc_color,
            pix,
        );
    }

    if let Some(sub) = geom.submenu {
        draw_caret(sub.x + sub.w * 0.5, sub.y, pix);
        if let Some(r) = Rect::from_xywh(sub.x, sub.y, sub.w, sub.h) {
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            if let Some(p) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color_rgba8(0, 0, 0, 255);
                paint.anti_alias = true;
                pix.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
        let sub_shapes = [
            &icons.shape_ray,
            &icons.shape_line,
            &icons.shape_rect,
            &icons.shape_circle,
        ];
        let sub_keys = ["a", "s", "d", "f"];
        for (i, sb) in geom.submenu_shapes.iter().enumerate() {
            let c = sb.center();
            let scale = 28.0 / 21.0;
            let ts = Transform::from_translate(c.x - 10.5 * scale, c.y - 10.5 * scale)
                .pre_scale(scale, scale);
            let mut p = Paint::default();
            p.set_color(Color::WHITE);
            p.anti_alias = true;
            let s = Stroke {
                width: 0.9,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            pix.stroke_path(sub_shapes[i], &p, &s, ts, None);
            let b_offset = if i > 0 { -2.0 } else { 0.0 };
            text.render_text(
                sub_keys[i],
                sb.x + sb.w - 12.0 - b_offset,
                sb.y + sb.h - 10.0,
                Color::from_rgba8(0x62, 0x62, 0x62, 255),
                pix,
            );
        }
    }
}
