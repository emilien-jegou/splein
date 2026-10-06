// Single responsibility: Layer-shell overlay surface setup for Aurora Wayland embedding.
//
// Thin, state-generic wrappers over `smithay-client-toolkit`: fullscreen overlay layer,
// click-through input regions, and configure handling. Frame pacing (configure guards,
// `waiting_for_frame` coalescing) stays with the embedder's event loop; rasterization
// and commit live in [`WaylandBackend`](crate::backend::WaylandBackend).

use smithay_client_toolkit::compositor::{CompositorState, SurfaceData};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface, LayerSurfaceData,
};
use smithay_client_toolkit::shell::WaylandSurface;
use wayland_client::protocol::{wl_output, wl_region::WlRegion, wl_surface::WlSurface};
use wayland_client::{Dispatch, QueueHandle};

/// Creates a fullscreen click-through overlay layer surface and commits it.
///
/// Mirrors the overlay contract: pinned to all edges, exclusive zone -1 so nothing
/// is displaced, no keyboard interactivity, and an empty input region so the pointer
/// falls through until content opts in via [`set_passthrough`].
pub fn create_overlay_surface<State>(
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    output: Option<&wl_output::WlOutput>,
    qh: &QueueHandle<State>,
    namespace: &str,
) -> LayerSurface
where
    State: Dispatch<WlSurface, SurfaceData>
        + Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData>
        + Dispatch<WlRegion, ()>
        + 'static,
{
    let surface = compositor.create_surface(qh);
    let layer = layer_shell.create_layer_surface(
        qh,
        surface,
        Layer::Overlay,
        Some(namespace.to_string()),
        output,
    );

    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);

    let empty_region = compositor.wl_compositor().create_region(qh, ());
    layer.wl_surface().set_input_region(Some(&empty_region));
    empty_region.destroy();

    layer.commit();
    layer
}

/// Toggles pointer click-through on a layer surface.
///
/// Pass-through installs an empty input region (pointer falls to windows below);
/// opting in clears the region and requests on-demand keyboard interactivity.
pub fn set_passthrough<State>(
    compositor: &CompositorState,
    layer: &LayerSurface,
    qh: &QueueHandle<State>,
    passthrough: bool,
) where
    State: Dispatch<WlRegion, ()> + 'static,
{
    let wl_surf = layer.wl_surface();
    if passthrough {
        let empty_region = compositor.wl_compositor().create_region(qh, ());
        wl_surf.set_input_region(Some(&empty_region));
        empty_region.destroy();
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    } else {
        wl_surf.set_input_region(None);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    }
    wl_surf.commit();
}
