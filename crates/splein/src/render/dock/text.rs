// Shapes and rasterizes subpixel typography badges directly into tiny-skia.

use cosmic_text::{Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::{Color, PixmapMut};

pub struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl Default for TextEngine {
    fn default() -> Self {
        let mut db = cosmic_text::fontdb::Database::new();
        // Load system fonts cleanly without crawling user home folder junk
        db.load_fonts_dir("/usr/share/fonts");
        db.load_fonts_dir("/usr/local/share/fonts");

        let font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
        Self { font_system, swash_cache: SwashCache::new() }
    }
}

impl TextEngine {
    pub fn render_text(&mut self, text: &str, x: f32, y: f32, color: Color, pix: &mut PixmapMut) {
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(10.0, 10.0));
        buffer.set_text(&mut self.font_system, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let cosmic_color = CosmicColor::rgba(
            (color.red() * 255.0) as u8, (color.green() * 255.0) as u8,
            (color.blue() * 255.0) as u8, (color.alpha() * 255.0) as u8,
        );

        buffer.draw(&mut self.font_system, &mut self.swash_cache, cosmic_color, |gx, gy, _w, _h, c| {
            let px = (x as i32 + gx) as usize;
            let py = (y as i32 + gy) as usize;
            if px < pix.width() as usize && py < pix.height() as usize {
                let idx = (py * pix.width() as usize + px) * 4;
                let data = pix.data_mut();
                data[idx] = c.r();
                data[idx + 1] = c.g();
                data[idx + 2] = c.b();
                data[idx + 3] = c.a();
            }
        });
    }
}
