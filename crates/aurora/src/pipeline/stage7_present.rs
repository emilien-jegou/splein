// Single responsibility: Presentation stage forwarding damaged regions and buffer lifecycle.

use bon::bon;
use softbuffer::Buffer;
use tiny_skia::Pixmap;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::app::SurfacePresenter;
use crate::foundation::{DamageRegion, ResolvedRect};

/// Execution receipt confirming OS surface presentation.
pub struct PresentReceipt;

#[bon]
impl PresentReceipt {
    /// Presents the pixmap to the target OS surface displaying damaged regions.
    #[builder]
    pub fn execute<'a, D, W, F>(
        pixmap: &Pixmap,
        buffer: Buffer<'a, D, W>,
        damage: &DamageRegion,
        extra_damage: &[ResolvedRect],
        active_w: u32,
        active_h: u32,
        post_process: F,
    ) where
        D: HasDisplayHandle,
        W: HasWindowHandle,
        F: FnOnce(&mut [u32], &DamageRegion, usize, usize),
    {
        SurfacePresenter::present_damaged()
            .pixmap(pixmap)
            .buffer(buffer)
            .damage(damage)
            .extra_damage(extra_damage)
            .active_w(active_w)
            .active_h(active_h)
            .post_process(post_process)
            .call();
    }

    /// Fallback presentation without extra damage.
    #[builder]
    pub fn execute_simple<'a, D, W, F>(
        pixmap: &Pixmap,
        buffer: Buffer<'a, D, W>,
        damage: &DamageRegion,
        active_w: u32,
        active_h: u32,
        post_process: F,
    ) where
        D: HasDisplayHandle,
        W: HasWindowHandle,
        F: FnOnce(&mut [u32], &DamageRegion, usize, usize),
    {
        SurfacePresenter::present_damaged()
            .pixmap(pixmap)
            .buffer(buffer)
            .damage(damage)
            .extra_damage(&[])
            .active_w(active_w)
            .active_h(active_h)
            .post_process(post_process)
            .call();
    }
}
