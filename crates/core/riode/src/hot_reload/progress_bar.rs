// Single responsibility: BProgress-style top loading bar shown while the dev
// loop is rebuilding.
//
// The component and remote reload extensions own one of these and feed it the
// published [`DevLoopState`](crate::hot_reload::dev_loop::DevLoopState) plus the
// engine's simulated clock every `on_update`. It draws through the ordinary
// extension overlay seam (`render_overlay` / `overlay_damage`), so it rides
// above whatever the UI tree (or its placeholder) is showing, on every backend.
//
// Look: 3px bar pinned to the top edge, full window width. Black on light
// backgrounds, inverted to white on dark ones (picked from the window clear
// color each frame, so every demo reads correctly without configuration).
// Soft glow beneath (one blurred shadow layer), a brighter luminous leading
// edge, and a narrow pale shimmer sweeping the bar. No track, no spinner,
// no borders, no text.
//
// Motion contract:
// - While building, progress follows a two-phase ease: it accelerates from 0
//   to 70% (ease-in), then decelerates from 70% to a 90% hold (ease-out) with
//   matched velocity at the junction, so the whole rise is C1-smooth with peak
//   speed at 70%. Snaps 90% → 100% on swap, then fades out. A failed build
//   fades out too; the error itself surfaces through stats/HUD as before.
// - Everything is driven by engine *simulated* time, never wall time: `F8`
//   freeze holds the bar (and its shimmer) still with state intact, `[` / `]`
//   slow-motion scales it. That is this engine's form of reduced-motion
//   support — no animation advances while time doesn't.

use std::cell::Cell;

use crate::app::extension::OverlayCaps;
use crate::app::painter::OverlayPainter;
use crate::foundation::{Appearance, Color, ColorStop, Fill, LinearGradient, Point, ResolvedRect};
use crate::hot_reload::dev_loop::DevLoopState;
use crate::scene::SceneCommand;

/// Bar height, px. Everything else (glow, damage strip) derives from this.
const BAR_H: f32 = 5.0;
/// Glow extent below the bar, px (one blurred shadow layer, not a track).
const GLOW_EXTENT: f32 = 8.0;
/// Damage strip height: bar + glow + one pixel of slack.
const STRIP_H: f32 = BAR_H + GLOW_EXTENT + 2.0;
/// Hold level while building (BProgress parks just short of done); the last
/// 10% lands instantly on swap.
const BUILD_MAX: f32 = 0.90;
/// Level where the rise switches from accelerating to decelerating.
const EASE_PEAK: f32 = 0.70;
/// Time to accelerate 0 → 70%, seconds of simulated time.
const RISE_T: f32 = 0.5;
/// Time to decelerate 70% → 90%, seconds of simulated time. Chosen so the
/// ease-out's entry velocity matches the ease-in's exit velocity
/// (`2*EASE_PEAK/RISE_T == 2*(BUILD_MAX-EASE_PEAK)/EASE_TAIL_T`), keeping the
/// rise C1-smooth with peak speed exactly at 70%.
const EASE_TAIL_T: f32 = (BUILD_MAX - EASE_PEAK) * RISE_T / EASE_PEAK;
/// Snap-to-full duration once the swap lands, seconds.
const COMPLETE_SECS: f32 = 0.15;
/// Fade-out duration after reaching full (or on build failure), seconds.
const FADE_SECS: f32 = 0.30;
/// Shimmer sweep period across the bar, seconds.
const SHIMMER_PERIOD: f32 = 1.8;
const SHIMMER_W: f32 = 300.0;

/// Bar palette, flipped by background luminance so the bar reads on light and
/// dark demos alike. `bar_*` is the body gradient, `edge` the luminous head,
/// `glow` the shadow beneath, `shimmer` the sweeping band.
struct BarScheme {
    bar_lo: u32,
    bar_mid: u32,
    bar_hi: u32,
    edge: u32,
    glow: u32,
    shimmer: u32,
}

const DARK_SCHEME: BarScheme = BarScheme {
    bar_lo: 0xCC5500,
    bar_mid: 0xFF8800,
    bar_hi: 0xFFAA33,
    edge: 0xFFFFFF,
    glow: 0x000000,
    shimmer: 0x00FFFF,
};

const LIGHT_SCHEME: BarScheme = BarScheme {
    bar_lo: 0xFFAA33,
    bar_mid: 0xFF8800,
    bar_hi: 0xCC5500,
    edge: 0x111111,
    glow: 0xFFFFFF,
    shimmer: 0x00FFFF,
};

/// Relative luminance (Rec. 709) of a clear color, 0 = black, 1 = white.
fn luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

/// Picks the contrasting scheme: black bar on light windows, white on dark.
fn scheme_for(background: Color) -> &'static BarScheme {
    if luminance(background) > 0.5 {
        &DARK_SCHEME
    } else {
        &LIGHT_SCHEME
    }
}

#[derive(Default)]
enum Phase {
    #[default]
    Hidden,
    /// A build is in flight (or sources went stale and one is imminent).
    Active { start_sim: f32 },
    /// Swap landed: snap the trickle value to full.
    Completing { from: f32, start_sim: f32 },
    /// Full (or failed): fade the whole bar out.
    Fading { start_sim: f32 },
}

/// Top loading bar for one dev-loop extension. See the module docs.
pub struct ReloadProgressBar {
    phase: Phase,
    progress: Cell<f32>,
    alpha: Cell<f32>,
    sweep: Cell<f32>,
    width: Cell<f32>,
    /// Set when the bar hides: the next `damage()` call reports the strip one
    /// last time so the final bar pixels repaint, then latches off.
    clear_pending: Cell<bool>,
}

impl ReloadProgressBar {
    pub fn new() -> Self {
        Self {
            phase: Phase::Hidden,
            progress: Cell::new(0.0),
            alpha: Cell::new(0.0),
            sweep: Cell::new(0.0),
            width: Cell::new(0.0),
            clear_pending: Cell::new(false),
        }
    }

    /// Advances the bar from the published loop state. Returns true while the
    /// bar needs frames (visible or fading), so the extension keeps waking.
    pub fn update(&mut self, state: DevLoopState, sim: f32, viewport_w: f32) -> bool {
        match self.phase {
            Phase::Hidden => {
                if matches!(state, DevLoopState::Building | DevLoopState::Stale) {
                    self.phase = Phase::Active { start_sim: sim };
                }
            }
            Phase::Active { start_sim } => match state {
                DevLoopState::Watching => {
                    self.phase = Phase::Completing {
                        from: trickle(sim - start_sim),
                        start_sim: sim,
                    };
                }
                DevLoopState::Failed => {
                    self.phase = Phase::Fading { start_sim: sim };
                }
                _ => {}
            },
            Phase::Completing { start_sim, .. } => {
                if sim - start_sim >= COMPLETE_SECS {
                    self.phase = Phase::Fading { start_sim: sim };
                }
            }
            Phase::Fading { start_sim } => {
                if sim - start_sim >= FADE_SECS {
                    self.phase = Phase::Hidden;
                    // The scene still holds our last chunk: one final damage
                    // report clears it, then the bar costs nothing.
                    self.clear_pending.set(true);
                }
            }
        }
        let (progress, alpha) = match self.phase {
            Phase::Hidden => (0.0, 0.0),
            Phase::Active { start_sim } => (trickle(sim - start_sim), 1.0),
            Phase::Completing { from, start_sim } => {
                let t = ((sim - start_sim) / COMPLETE_SECS).clamp(0.0, 1.0);
                (from + (1.0 - from) * ease_out(t), 1.0)
            }
            Phase::Fading { start_sim } => {
                let t = ((sim - start_sim) / FADE_SECS).clamp(0.0, 1.0);
                (1.0, 1.0 - t)
            }
        };
        self.progress.set(progress);
        self.alpha.set(alpha);
        self.width.set(viewport_w);
        // Shimmer rides simulated time: frozen/slowed clock stills/scales it.
        self.sweep.set(sim);
        alpha > 0.0
    }

    /// Damaged screen bounds for the bar — plus one last report after hiding so
    /// the final bar pixels repaint instead of lingering.
    pub fn damage(&self) -> Option<ResolvedRect> {
        let w = self.width.get();
        if w <= 0.0 {
            return None;
        }
        if self.alpha.get() > 0.0 {
            return Some(ResolvedRect::new(0.0, 0.0, w, STRIP_H));
        }
        if self.clear_pending.get() {
            self.clear_pending.set(false);
            return Some(ResolvedRect::new(0.0, 0.0, w, STRIP_H));
        }
        None
    }

    /// Records the bar chunks. No-op unless visible.
    pub fn draw(&self, paint: &mut OverlayPainter<'_>, caps: &OverlayCaps) {
        let alpha = self.alpha.get();
        if alpha <= 0.0 {
            return;
        }
        let w = caps.viewport.width;
        if w <= 0.0 {
            return;
        }
        let bar_w = (self.progress.get().clamp(0.0, 1.0) * w).max(2.0);
        let scheme = scheme_for(caps.background);
        paint.record(ResolvedRect::new(0.0, 0.0, w, STRIP_H), |chunk| {
            // 1. Soft glow beneath: the bar silhouette blurred downward.
            chunk.push(SceneCommand::DrawRect {
                rect: ResolvedRect::new(0.0, 0.0, bar_w, BAR_H),
                appearance: Appearance::EMPTY
                    .with_fill(Fill::solid(Color::hex_alpha(scheme.bar_lo, 0.0)))
                    .with_shadow(crate::foundation::Shadow::web_like(
                        0.0,
                        2.0,
                        GLOW_EXTENT,
                        0.0,
                        Color::hex_alpha(scheme.glow, 0.55),
                    ))
                    .with_opacity(alpha),
            });
            // 2. The gradient bar itself, stable across the full width so
            // growth reveals more of the same gradient instead of stretching it.
            chunk.push(SceneCommand::DrawRect {
                rect: ResolvedRect::new(0.0, 0.0, bar_w, BAR_H),
                appearance: Appearance::EMPTY
                    .with_fill(Fill::LinearGradient(LinearGradient {
                        start: Point::new(0.0, 0.0),
                        end: Point::new(w, 0.0),
                        stops: vec![
                            ColorStop {
                                position: 0.0,
                                color: Color::hex(scheme.bar_lo),
                            },
                            ColorStop {
                                position: 0.5,
                                color: Color::hex(scheme.bar_mid),
                            },
                            ColorStop {
                                position: 1.0,
                                color: Color::hex(scheme.bar_hi),
                            },
                        ],
                    }))
                    .with_opacity(alpha),
            });
            // 3. Glass shimmer: a narrow translucent band sweeping the bar,
            // pale on dark bars, dark on light ones.
            let sweep_x = sweep_pos(self.sweep.get(), w);
            let band_x0 = (sweep_x - SHIMMER_W).max(0.0).min(bar_w);
            let band_x1 = sweep_x.max(0.0).min(bar_w);
            if band_x1 > band_x0 {
                chunk.push(SceneCommand::DrawRect {
                    rect: ResolvedRect::new(band_x0, 0.0, band_x1 - band_x0, BAR_H),
                    appearance: Appearance::EMPTY
                        .with_fill(Fill::LinearGradient(LinearGradient {
                            start: Point::new(band_x0, 0.0),
                            end: Point::new(band_x1, 0.0),
                            stops: vec![
                                ColorStop {
                                    position: 0.0,
                                    color: Color::hex_alpha(scheme.shimmer, 0.0),
                                },
                                ColorStop {
                                    position: 0.5,
                                    color: Color::hex_alpha(scheme.shimmer, 0.38),
                                },
                                ColorStop {
                                    position: 1.0,
                                    color: Color::hex_alpha(scheme.shimmer, 0.0),
                                },
                            ],
                        }))
                        .with_opacity(alpha),
                });
            }
            // 4. Luminous leading edge: a brighter head on the progress front.
            let edge_w = 3.0_f32.min(bar_w);
            chunk.push(SceneCommand::DrawRect {
                rect: ResolvedRect::new(bar_w - edge_w, 0.0, edge_w, BAR_H),
                appearance: Appearance::EMPTY
                    .with_fill(Fill::solid(Color::hex(scheme.edge)))
                    .with_shadow(crate::foundation::Shadow::web_like(
                        0.0,
                        0.0,
                        4.0,
                        1.0,
                        Color::hex_alpha(scheme.edge, 0.8),
                    ))
                    .with_opacity(alpha),
            });
        });
    }
}

/// Build-rise easing: ease-in quad 0 → 70% over `RISE_T`, then ease-out quad
/// 70% → 90% over `EASE_TAIL_T`, holding at 90% past that. Peak speed lands
/// exactly at 70% with matched velocity across the junction.
fn trickle(elapsed: f32) -> f32 {
    let t = elapsed.max(0.0);
    if t < RISE_T {
        let x = t / RISE_T;
        return EASE_PEAK * x * x;
    }
    let u = ((t - RISE_T) / EASE_TAIL_T).clamp(0.0, 1.0);
    EASE_PEAK + (BUILD_MAX - EASE_PEAK) * (1.0 - (1.0 - u) * (1.0 - u))
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Shimmer head position: loops across the bar plus one band width of run-off.
fn sweep_pos(sim: f32, width: f32) -> f32 {
    let span = width + SHIMMER_W;
    if span <= 0.0 {
        return 0.0;
    }
    let phase = (sim / SHIMMER_PERIOD).max(0.0) % 1.0;
    phase * span
}
