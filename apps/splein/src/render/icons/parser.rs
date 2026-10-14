// Parses minimal SVG path command syntax into tiny-skia Path structures.

use tiny_skia::{Path, PathBuilder};

pub fn parse_svg(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let bytes = d.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let cmd = bytes[i];
        if cmd.is_ascii_alphabetic() {
            i += 1;
        }

        match cmd {
            b'M' | b'L' => {
                if let (Some(x), Some(y)) = (next_f32(bytes, &mut i), next_f32(bytes, &mut i)) {
                    if cmd == b'M' {
                        pb.move_to(x, y);
                    } else {
                        pb.line_to(x, y);
                    }
                }
            }
            b'C' => {
                while let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    next_f32(bytes, &mut i),
                    next_f32(bytes, &mut i),
                    next_f32(bytes, &mut i),
                    next_f32(bytes, &mut i),
                    next_f32(bytes, &mut i),
                    next_f32(bytes, &mut i),
                ) {
                    pb.cubic_to(x1, y1, x2, y2, x, y);
                    if i >= bytes.len() || bytes[i].is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            b'Z' | b'z' => {
                pb.close();
            }
            _ => {
                i += 1;
            }
        }
    }
    pb.finish()
}

fn next_f32(b: &[u8], i: &mut usize) -> Option<f32> {
    while *i < b.len() && (b[*i].is_ascii_whitespace() || b[*i] == b',') {
        *i += 1;
    }
    let s = *i;
    if *i < b.len() && (b[*i] == b'+' || b[*i] == b'-') {
        *i += 1;
    }
    while *i < b.len() && (b[*i].is_ascii_digit() || b[*i] == b'.') {
        *i += 1;
    }
    std::str::from_utf8(&b[s..*i]).ok()?.parse().ok()
}
