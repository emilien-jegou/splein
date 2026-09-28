// crates/splein/src/render/icons.rs

use std::sync::OnceLock;
use tiny_skia::{Color, LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Stroke, Transform};

pub const SVG_DRAG_HANDLE: &str = "M13.125 15.75C13.125 16.233 12.733 16.625 12.25 16.625C11.767 16.625 11.375 16.233 11.375 15.75C11.375 15.267 11.767 14.875 12.25 14.875C12.733 14.875 13.125 15.267 13.125 15.75M8.75 10.5C8.75 10.983 8.358 11.375 7.875 11.375C7.392 11.375 7 10.983 7 10.5C7 10.017 7.392 9.625 7.875 9.625C8.358 9.625 8.75 10.017 8.75 10.5M13.125 5.25C13.125 5.733 12.733 6.125 12.25 6.125C11.767 6.125 11.375 5.733 11.375 5.25C11.375 4.767 11.767 4.375 12.25 4.375C12.733 4.375 13.125 4.767 13.125 5.25M13.125 10.5C13.125 10.983 12.733 11.375 12.25 11.375C11.767 11.375 11.375 10.983 11.375 10.5C11.375 10.017 11.767 9.625 12.25 9.625C12.733 9.625 13.125 10.017 13.125 10.5M8.75 5.25C8.75 5.733 8.358 6.125 7.875 6.125C7.392 6.125 7 5.733 7 5.25C7 4.767 7.392 4.375 7.875 4.375C8.358 4.375 8.75 4.767 8.75 5.25M8.75 15.75C8.75 16.233 8.358 16.625 7.875 16.625C7.392 16.625 7 16.233 7 15.75C7 15.267 7.392 14.875 7.875 14.875C8.358 14.875 8.75 15.267 8.75 15.75";
pub const SVG_TOOL_Q: &str = "M8.989 5.434l4.148 1.622c2.392 0.937 3.589 1.405 3.549 2.147-0.041 0.742-1.288 1.084-3.785 1.764-0.743 0.203-1.115 0.305-1.373 0.561s-0.359 0.63-0.561 1.373c-0.68 2.497-1.022 3.745-1.764 3.785s-1.21-1.156-2.147-3.549L5.434 8.989C4.453 6.484 3.963 5.233 4.598 4.598c0.635-0.635 1.888-0.145 4.391 0.836Z";
pub const SVG_TOOL_W: &str = "M12.907 5.353c0.504-0.504 0.756-0.756 1.036-0.878 0.404-0.174 0.86-0.174 1.263 0 0.28 0.12 0.532 0.373 1.036 0.878s0.756 0.757 0.878 1.036c0.174 0.404 0.174 0.86 0 1.263-0.12 0.28-0.373 0.532-0.878 1.036l-4.316 4.318c-1.064 1.064-1.595 1.595-2.261 1.91s-1.414 0.389-2.912 0.537L6.076 15.519l0.067-0.677c0.148-1.497 0.222-2.246 0.536-2.912s0.847-1.198 1.91-2.261z";
pub const SVG_TOOL_E: &str = "M7.075 12.83l1.473 1.473m-1.473-1.473L4.165 15.82h2.786l1.597-1.517m-1.473-1.473c-0.271-0.271-0.266-0.711-0.031-1.013 0.552-0.706 0.753-1.345 0.811-1.821 0.065-0.524 0.191-1.09 0.565-1.463l0.621-0.619m-0.493 6.389c0.271 0.271 0.711 0.266 1.013 0.031 0.706-0.552 1.345-0.753 1.821-0.811 0.524-0.065 1.09-0.191 1.463-0.565l0.62-0.62m0 0l-4.423-4.423m4.423 4.423a0.697 0.697 0 0 0 0.983 0l2.949-2.95M9.041 7.914a0.697 0.697 0 0 1 0-0.984L11.99 3.981";
pub const SVG_TOOL_R: &str = "M15.145 5.142L15.145 15.859M11.214 14.072L9.836 10.857M16.93 16.93C16.335 16.934 15.5 16.573 15.145 15.859M4.071 14.072L5.448 10.857M15.145 15.859C14.786 16.573 13.953 16.93 13.358 16.93M5.448 10.857L6.944 7.366C7.081 7.05 7.319 6.928 7.643 6.928C7.967 6.928 8.206 7.05 8.341 7.366L9.836 10.857M16.216 10.5L14.072 10.5M5.448 10.857L9.836 10.857M13.358 4.071C13.955 4.066 14.788 4.428 15.145 5.142M15.145 5.142C15.502 4.426 16.336 4.071 16.93 4.071";
pub const SVG_TOOL_T_1: &str = "M12.879 11.487l2.542 0.995c1.467 0.573 2.199 0.86 2.175 1.315-0.023 0.455-0.79 0.665-2.32 1.082-0.455 0.125-0.684 0.188-0.84 0.344-0.158 0.158-0.222 0.386-0.345 0.84-0.417 1.531-0.625 2.296-1.08 2.321-0.455 0.025-0.742-0.708-1.316-2.175l-0.997-2.543c-0.601-1.535-0.901-2.302-0.511-2.691 0.39-0.389 1.156-0.089 2.693 0.511Z";
pub const SVG_TOOL_T_2: &str = "M3.675 9.165v2.172M10.549 4.463h-2.171m0 11.576H8.74m6.512-6.512v-0.362M5.485 16.039A1.808 1.808 90 0 1 3.675 14.231m0-7.959A1.808 1.808 90 0 1 5.485 4.463M15.252 6.272A1.808 1.808 90 0 0 13.444 4.463";
pub const SVG_TOOL_Y: &str = "M4.396 15.926L16.604 5.074";
pub const SVG_TOOL_U: &str = "M4.38 10.5c0-2.884 0-4.327 0.896-5.224S7.615 4.38 10.5 4.38c2.884 0 4.327 0 5.224 0.896S16.62 7.615 16.62 10.5c0 2.884 0 4.327-0.896 5.224S13.385 16.62 10.5 16.62c-2.884 0-4.327 0-5.224-0.896S4.38 13.385 4.38 10.5Z";
pub const SVG_TOOL_O: &str = "M8.293 7.645l-2.631 2.691c-0.791 0.809-1.187 1.213-1.24 1.706q-0.018 0.158 0 0.314c0.054 0.494 0.449 0.897 1.24 1.707l0.1 0.103c0.422 0.432 0.634 0.648 0.886 0.792q0.222 0.127 0.466 0.198c0.28 0.079 0.579 0.079 1.179 0.08 0.599 0 0.901 0 1.18-0.08q0.245-0.07 0.465-0.197c0.252-0.145 0.464-0.362 0.886-0.793l1.924-1.967M8.293 7.645l2.424-2.473C11.654 4.214 12.124 3.736 12.707 3.736s1.052 0.48 1.99 1.437l0.502 0.515C16.126 6.633 16.588 7.106 16.588 7.692s-0.463 1.059-1.389 2.005l-2.451 2.502M8.293 7.645l4.455 4.554M9.147 17.264h7.441";
pub const SVG_ACTION_TRASH: &str = "M15.492 6.173l-0.413 6.673c-0.105 1.705-0.158 2.557-0.585 3.171a2.662 2.662 90 0 1-0.798 0.75c-0.637 0.389-1.491 0.389-3.199 0.39-1.71 0-2.566 0-3.205-0.39a2.662 2.662 90 0 1-0.799-0.752c-0.428-0.614-0.48-1.468-0.581-3.176L5.508 6.173M4.509 6.173h11.982m-3.291 0l-0.454-0.937c-0.302-0.623-0.453-0.933-0.714-1.128a1.331 1.331 90 0 0-0.183-0.114C11.561 3.843 11.215 3.843 10.523 3.843c-0.711 0-1.064 0-1.358 0.156a1.331 1.331 90 0 0-0.184 0.121c-0.263 0.202-0.41 0.525-0.705 1.169L7.873 6.173";
pub const SVG_ACTION_MORE: &str = "M10.498 10.938V10.5m0-4.812V5.25m0 10.938V15.75m0.875-4.812a0.875 0.875 0 1 0-1.75 0 0.875 0.875 0 0 0 1.75 0m0-5.25a0.875 0.875 0 1 0-1.75 0 0.875 0.875 0 0 0 1.75 0m0 10.5a0.875 0.875 0 1 0-1.75 0 0.875 0.875 0 0 0 1.75 0";

pub struct IconCache {
    pub drag_handle: Path,
    pub tool_q: Path,
    pub tool_w: Path,
    pub tool_e: Path,
    pub tool_r: Path,
    pub tool_t_1: Path,
    pub tool_t_2: Path,
    pub tool_y: Path,
    pub tool_u: Path,
    pub tool_o: Path,
    pub action_trash: Path,
    pub action_more: Path,
}

static ICONS: OnceLock<IconCache> = OnceLock::new();

fn fallback_empty_path() -> Path {
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, 0.0);
    pb.line_to(0.1, 0.1);
    pb.finish().unwrap()
}

pub fn get_icons() -> &'static IconCache {
    ICONS.get_or_init(|| IconCache {
        drag_handle: parse_svg_path(SVG_DRAG_HANDLE).unwrap_or_else(fallback_empty_path),
        tool_q: parse_svg_path(SVG_TOOL_Q).unwrap_or_else(fallback_empty_path),
        tool_w: parse_svg_path(SVG_TOOL_W).unwrap_or_else(fallback_empty_path),
        tool_e: parse_svg_path(SVG_TOOL_E).unwrap_or_else(fallback_empty_path),
        tool_r: parse_svg_path(SVG_TOOL_R).unwrap_or_else(fallback_empty_path),
        tool_t_1: parse_svg_path(SVG_TOOL_T_1).unwrap_or_else(fallback_empty_path),
        tool_t_2: parse_svg_path(SVG_TOOL_T_2).unwrap_or_else(fallback_empty_path),
        tool_y: parse_svg_path(SVG_TOOL_Y).unwrap_or_else(fallback_empty_path),
        tool_u: parse_svg_path(SVG_TOOL_U).unwrap_or_else(fallback_empty_path),
        tool_o: parse_svg_path(SVG_TOOL_O).unwrap_or_else(fallback_empty_path),
        action_trash: parse_svg_path(SVG_ACTION_TRASH).unwrap_or_else(fallback_empty_path),
        action_more: parse_svg_path(SVG_ACTION_MORE).unwrap_or_else(fallback_empty_path),
    })
}

pub fn render_cached_icon_21(
    pixmap: &mut PixmapMut,
    path: &Path,
    center_x: f32,
    center_y: f32,
    target_size: f32,
    color: Color,
    stroke_width: f32,
) {
    let scale = target_size / 21.0;
    let transform = Transform::from_translate(center_x - 10.5 * scale, center_y - 10.5 * scale)
        .pre_scale(scale, scale);

    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;

    let stroke = Stroke {
        width: stroke_width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };

    pixmap.stroke_path(path, &paint, &stroke, transform, None);
}

pub fn parse_svg_path(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let bytes = d.as_bytes();
    let mut i = 0;
    let mut cur_x = 0.0f32;
    let mut cur_y = 0.0f32;
    let mut last_cmd = b' ';

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let loop_start_i = i;
        let ch = bytes[i];
        let cmd = if ch.is_ascii_alphabetic() {
            i += 1;
            last_cmd = ch;
            ch
        } else {
            if last_cmd == b'M' { b'L' } else if last_cmd == b'm' { b'l' } else { last_cmd }
        };

        match cmd {
            b'M' => {
                if let (Some(x), Some(y)) = (next_num(bytes, &mut i), next_num(bytes, &mut i)) {
                    cur_x = x;
                    cur_y = y;
                    pb.move_to(x, y);
                }
            }
            b'm' => {
                if let (Some(dx), Some(dy)) = (next_num(bytes, &mut i), next_num(bytes, &mut i)) {
                    cur_x += dx;
                    cur_y += dy;
                    pb.move_to(cur_x, cur_y);
                }
            }
            b'L' => {
                if let (Some(x), Some(y)) = (next_num(bytes, &mut i), next_num(bytes, &mut i)) {
                    cur_x = x;
                    cur_y = y;
                    pb.line_to(x, y);
                }
            }
            b'l' => {
                if let (Some(dx), Some(dy)) = (next_num(bytes, &mut i), next_num(bytes, &mut i)) {
                    cur_x += dx;
                    cur_y += dy;
                    pb.line_to(cur_x, cur_y);
                }
            }
            b'H' => {
                if let Some(x) = next_num(bytes, &mut i) {
                    cur_x = x;
                    pb.line_to(cur_x, cur_y);
                }
            }
            b'h' => {
                if let Some(dx) = next_num(bytes, &mut i) {
                    cur_x += dx;
                    pb.line_to(cur_x, cur_y);
                }
            }
            b'V' => {
                if let Some(y) = next_num(bytes, &mut i) {
                    cur_y = y;
                    pb.line_to(cur_x, cur_y);
                }
            }
            b'v' => {
                if let Some(dy) = next_num(bytes, &mut i) {
                    cur_y += dy;
                    pb.line_to(cur_x, cur_y);
                }
            }
            b'C' => {
                while let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                ) {
                    pb.cubic_to(x1, y1, x2, y2, x, y);
                    cur_x = x;
                    cur_y = y;
                    if i >= bytes.len() || bytes[i].is_ascii_alphabetic() { break; }
                }
            }
            b'c' => {
                while let (Some(dx1), Some(dy1), Some(dx2), Some(dy2), Some(dx), Some(dy)) = (
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                ) {
                    pb.cubic_to(cur_x + dx1, cur_y + dy1, cur_x + dx2, cur_y + dy2, cur_x + dx, cur_y + dy);
                    cur_x += dx;
                    cur_y += dy;
                    if i >= bytes.len() || bytes[i].is_ascii_alphabetic() { break; }
                }
            }
            b'Q' => {
                while let (Some(x1), Some(y1), Some(x), Some(y)) = (
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                ) {
                    pb.quad_to(x1, y1, x, y);
                    cur_x = x;
                    cur_y = y;
                    if i >= bytes.len() || bytes[i].is_ascii_alphabetic() { break; }
                }
            }
            b'q' => {
                while let (Some(dx1), Some(dy1), Some(dx), Some(dy)) = (
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                ) {
                    pb.quad_to(cur_x + dx1, cur_y + dy1, cur_x + dx, cur_y + dy);
                    cur_x += dx;
                    cur_y += dy;
                    if i >= bytes.len() || bytes[i].is_ascii_alphabetic() { break; }
                }
            }
            b'A' | b'a' => {
                while let (Some(rx), Some(ry), Some(phi), Some(large_arc), Some(sweep), Some(x), Some(y)) = (
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i), next_num(bytes, &mut i),
                    next_num(bytes, &mut i),
                ) {
                    let dest_x = if cmd == b'a' { cur_x + x } else { x };
                    let dest_y = if cmd == b'a' { cur_y + y } else { y };
                    arc_to_cubics(
                        &mut pb,
                        cur_x,
                        cur_y,
                        rx,
                        ry,
                        phi,
                        large_arc != 0.0,
                        sweep != 0.0,
                        dest_x,
                        dest_y,
                    );
                    cur_x = dest_x;
                    cur_y = dest_y;
                    if i >= bytes.len() || bytes[i].is_ascii_alphabetic() { break; }
                }
            }
            b'Z' | b'z' => {
                pb.close();
            }
            _ => {
                i += 1;
            }
        }

        // Hard safety valve: guarantee progress to prevent infinite loops
        if i <= loop_start_i {
            i = loop_start_i + 1;
        }
    }

    pb.finish()
}

fn arc_to_cubics(
    pb: &mut PathBuilder,
    x1: f32, y1: f32,
    rx: f32, ry: f32,
    phi_deg: f32,
    large_arc: bool,
    sweep: bool,
    x2: f32, y2: f32,
) {
    if (x1 - x2).abs() < 1e-4 && (y1 - y2).abs() < 1e-4 {
        return;
    }
    let rx = rx.abs();
    let ry = ry.abs();
    if rx < 1e-4 || ry < 1e-4 {
        pb.line_to(x2, y2);
        return;
    }

    let phi = phi_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1_p = cos_phi * dx + sin_phi * dy;
    let y1_p = -sin_phi * dx + cos_phi * dy;

    let mut rx = rx;
    let mut ry = ry;
    let lambda = (x1_p * x1_p) / (rx * rx) + (y1_p * y1_p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let num = (rx * rx * ry * ry) - (rx * rx * y1_p * y1_p) - (ry * ry * x1_p * x1_p);
    let denom = (rx * rx * y1_p * y1_p) + (ry * ry * x1_p * x1_p);
    let factor = if denom > 1e-6 {
        sign * ((num.max(0.0)) / denom).sqrt()
    } else {
        0.0
    };

    let cx_p = factor * (rx * y1_p) / ry;
    let cy_p = factor * -(ry * x1_p) / rx;

    let cx = cos_phi * cx_p - sin_phi * cy_p + (x1 + x2) / 2.0;
    let cy = sin_phi * cx_p + cos_phi * cy_p + (y1 + y2) / 2.0;

    let ux = (x1_p - cx_p) / rx;
    let uy = (y1_p - cy_p) / ry;
    let vx = (-x1_p - cx_p) / rx;
    let vy = (-y1_p - cy_p) / ry;

    let theta1 = angle_between(1.0, 0.0, ux, uy);
    let mut d_theta = angle_between(ux, uy, vx, vy);
    if !sweep && d_theta > 0.0 {
        d_theta -= std::f32::consts::TAU;
    } else if sweep && d_theta < 0.0 {
        d_theta += std::f32::consts::TAU;
    }

    // Bound maximum segments to prevent division-by-zero loop traps
    let num_segments = (d_theta.abs() / std::f32::consts::FRAC_PI_2).ceil().clamp(1.0, 16.0) as usize;
    let dt = d_theta / num_segments as f32;

    let mut current_theta = theta1;
    for _ in 0..num_segments {
        let next_theta = current_theta + dt;
        let alpha = (dt / 4.0).tan() * 4.0 / 3.0;

        let cos_t1 = current_theta.cos();
        let sin_t1 = current_theta.sin();
        let cos_t2 = next_theta.cos();
        let sin_t2 = next_theta.sin();

        let p1x = rx * cos_t1;
        let p1y = ry * sin_t1;
        let p2x = rx * cos_t2;
        let p2y = ry * sin_t2;

        let q1x = p1x - alpha * rx * sin_t1;
        let q1y = p1y + alpha * ry * cos_t1;
        let q2x = p2x + alpha * rx * sin_t2;
        let q2y = p2y - alpha * ry * cos_t2;

        let c1x = cos_phi * q1x - sin_phi * q1y + cx;
        let c1y = sin_phi * q1x + cos_phi * q1y + cy;
        let c2x = cos_phi * q2x - sin_phi * q2y + cx;
        let c2y = sin_phi * q2x + cos_phi * q2y + cy;
        let end_x = cos_phi * p2x - sin_phi * p2y + cx;
        let end_y = sin_phi * p2x + cos_phi * p2y + cy;

        pb.cubic_to(c1x, c1y, c2x, c2y, end_x, end_y);
        current_theta = next_theta;
    }
}

fn angle_between(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
    let dot = ux * vx + uy * vy;
    let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    if len < 1e-6 {
        return 0.0;
    }
    let cos = (dot / len).clamp(-1.0, 1.0);
    let angle = cos.acos();
    if (ux * vy - uy * vx) < 0.0 {
        -angle
    } else {
        angle
    }
}

fn next_num(bytes: &[u8], i: &mut usize) -> Option<f32> {
    while *i < bytes.len() && (bytes[*i] == b' ' || bytes[*i] == b',' || bytes[*i] == b'\t' || bytes[*i] == b'\n' || bytes[*i] == b'\r') {
        *i += 1;
    }
    if *i >= bytes.len() {
        return None;
    }

    let start = *i;
    if bytes[*i] == b'+' || bytes[*i] == b'-' {
        *i += 1;
    }

    let mut has_digit = false;
    let mut has_dot = false;

    while *i < bytes.len() {
        let b = bytes[*i];
        if b.is_ascii_digit() {
            has_digit = true;
            *i += 1;
        } else if b == b'.' && !has_dot {
            has_dot = true;
            *i += 1;
        } else {
            break;
        }
    }

    if !has_digit {
        *i = start;
        return None;
    }

    if *i < bytes.len() && (bytes[*i] == b'e' || bytes[*i] == b'E') {
        *i += 1;
        if *i < bytes.len() && (bytes[*i] == b'+' || bytes[*i] == b'-') {
            *i += 1;
        }
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
    }

    let s = std::str::from_utf8(&bytes[start..*i]).ok()?;
    s.parse::<f32>().ok()
}
