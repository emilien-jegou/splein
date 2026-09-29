// Drives smooth cubic-bezier easing for backdrop movement between tools.

pub struct SlideMotion {
    pub current_rel_x: f32,
    pub current_w: f32,
    start_x: f32,
    start_w: f32,
    target_x: f32,
    target_w: f32,
    progress: f32,
}

impl Default for SlideMotion {
    fn default() -> Self {
        Self { current_rel_x: 56.0, current_w: 42.0, start_x: 56.0, start_w: 42.0, target_x: 56.0, target_w: 42.0, progress: 1.0 }
    }
}

impl SlideMotion {
    pub fn set_target(&mut self, rel_x: f32, w: f32) {
        if (self.target_x - rel_x).abs() > 0.5 || (self.target_w - w).abs() > 0.5 {
            self.start_x = self.current_rel_x;
            self.start_w = self.current_w;
            self.target_x = rel_x;
            self.target_w = w;
            self.progress = 0.0;
        }
    }

    pub fn is_animating(&self) -> bool { self.progress < 1.0 }

    pub fn tick(&mut self, dt_secs: f32) -> bool {
        if self.progress >= 1.0 { return false; }
        self.progress = (self.progress + dt_secs / 0.18).min(1.0);
        let ease = 1.0 - (1.0 - self.progress).powi(3);
        self.current_rel_x = self.start_x + (self.target_x - self.start_x) * ease;
        self.current_w = self.start_w + (self.target_w - self.start_w) * ease;
        true
    }
}
