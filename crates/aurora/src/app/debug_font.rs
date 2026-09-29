// Single responsibility: Zero-allocation scalable micro-font bitmap glyph rasterizer.

/// Renders a single 3x5 font glyph into the destination pixel buffer.
pub fn draw_char_scaled(
    buffer: &mut [u32],
    c: char,
    x0: usize,
    y0: usize,
    scale: usize,
    color: u32,
    stride: usize,
    height: usize,
) {
    let mask: u16 = match c.to_ascii_uppercase() {
        '0' => 0b111_101_101_101_111, '1' => 0b010_110_010_010_111, '2' => 0b111_001_111_100_111,
        '3' => 0b111_001_111_001_111, '4' => 0b101_101_111_001_001, '5' => 0b111_100_111_001_111,
        '6' => 0b111_100_111_101_111, '7' => 0b111_001_010_010_010, '8' => 0b111_101_111_101_111,
        '9' => 0b111_101_111_001_111, 'A' => 0b010_101_111_101_101, 'B' => 0b110_101_110_101_110,
        'C' => 0b111_100_100_100_111, 'D' => 0b110_101_101_101_110, 'E' => 0b111_100_111_100_111,
        'F' => 0b111_100_110_100_100, 'G' => 0b111_100_101_101_111, 'H' => 0b101_101_111_101_101,
        'I' => 0b111_010_010_010_111, 'J' => 0b001_001_001_101_010, 'K' => 0b101_101_110_101_101,
        'L' => 0b100_100_100_100_111, 'M' => 0b101_111_101_101_101, 'N' => 0b110_101_101_101_101,
        'O' => 0b111_101_101_101_111, 'P' => 0b111_101_111_100_100, 'Q' => 0b111_101_101_111_001,
        'R' => 0b110_101_110_101_101, 'S' => 0b111_100_111_001_111, 'T' => 0b111_010_010_010_010,
        'U' => 0b101_101_101_101_111, 'V' => 0b101_101_101_101_010, 'W' => 0b101_101_101_111_101,
        'X' => 0b101_101_010_101_101, 'Y' => 0b101_101_010_010_010, 'Z' => 0b111_001_010_100_111,
        '-' => 0b000_000_111_000_000, '(' | '[' => 0b010_100_100_100_010, ')' | ']' => 0b010_001_001_001_010,
        ':' => 0b000_010_000_010_000, '.' => 0b000_000_000_000_010, '+' => 0b000_010_111_010_000,
        '|' => 0b010_010_010_010_010, '/' => 0b001_001_010_100_100, ' ' => 0b000_000_000_000_000,
        _ => 0b111_111_111_111_111,
    };

    for row in 0..5 {
        for col in 0..3 {
            if ((mask >> (14 - (row * 3 + col))) & 1) == 1 {
                for sy in 0..scale {
                    let y = y0 + row * scale + sy;
                    if y >= height { break; }
                    for sx in 0..scale {
                        let x = x0 + col * scale + sx;
                        if x < stride { buffer[y * stride + x] = color; }
                    }
                }
            }
        }
    }
}

/// Renders a text string into the destination pixel buffer using bitmap glyphs.
pub fn draw_text_scaled(
    buffer: &mut [u32],
    text: &str,
    x0: usize,
    y0: usize,
    scale: usize,
    color: u32,
    stride: usize,
    height: usize,
) {
    let mut x = x0;
    for c in text.chars() {
        draw_char_scaled(buffer, c, x, y0, scale, color, stride, height);
        x += 4 * scale;
    }
}
