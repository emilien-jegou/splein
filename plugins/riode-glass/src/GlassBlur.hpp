#pragma once

// Single responsibility: plugin-owned backdrop blur (Phase 3a).
//
// One `CRenderPass::add` hook + one `IPassElement`. For every surface
// element whose `wl_surface` carries a non-empty
// `ext-background-effect-v1` blur region, the hook:
//   1. surgically disables native blur for exactly that draw
//      (`m_data.blur = false` on the queued element — no persistent
//      state, unload-safe: the next frame without the plugin blurs
//      natively again),
//   2. queues a `CGlassBlurElement` ahead of it, which samples the fresh
//      backdrop (`needsLiveBlur`), runs our own separable Gaussian, and
//      composites it back masked to the region rects.
//
// Damage citizenship comes from the two hints (`boundingBox` +
// `needsLiveBlur`), mirroring the reference. No redirect: the blur draws
// into `currentFB` BEFORE the surface content draws over it, so content
// always wins. Static-content staleness (backdrop moves, nothing damages)
// is a known 3a limitation — the demo animates every frame, so the gate
// cannot hit it; proper cure is the renderLayer redirect (3b).

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/render/pass/PassElement.hpp>

#include <hyprutils/math/Box.hpp>
#include <hyprutils/math/Region.hpp>
#include <hyprutils/math/Vector2D.hpp>

#include "GlassOptics.hpp"
#include "item-helper/riode_item_helper_api.h"

#include <memory>
#include <vector>

class CGlassBlurElement : public IPassElement {
  public:
    struct SData {
        // Region in surface-local logical px (copied at queue time).
        CRegion region;
        // Surface origin in monitor-local logical px (from the surface
        // element's draw position). Assumes surface scale == monitor
        // scale (true at 1x; the gate validates).
        Vector2D surfacePos;
        PHLMONITORREF monitor;
        float    monitorScale = 1.0f;
        float    radiusPx     = 14.0f; // Gaussian radius, framebuffer px
        // Optics (v5 packed frame): true when the island carries a packed
        // glass upload. Shape is island-local logical (hint space ==
        // region space for our islands); no packed hint falls back to
        // the Gaussian path.
        bool useOptics = false;
        // v5 pack: frame blur sigmas + integer sample box (x0, y0, w, h,
        // device px, used verbatim for blit/scissor/uSize) + the full
        // uniform floats (prefix + glow tail). Copied at queue time.
        float packSigmaX = 0.0f;
        float packSigmaY = 0.0f;
        float packBoxX = 0.0f;
        float packBoxY = 0.0f;
        float packBoxW = 0.0f;
        float packBoxH = 0.0f;
        float packed[RIODE_ITEM_HELPER_GLASS_PACKED_FLOATS] = {};
        CBox  shapeLocal{0, 0, 0, 0};
        float shapeRadii[4] = {};
    };

    explicit CGlassBlurElement(SData data);
    ~CGlassBlurElement() override = default;

    std::vector<UP<IPassElement>> draw() override;
    [[nodiscard]] bool                needsLiveBlur() override;
    [[nodiscard]] bool                needsPrecomputeBlur() override;
    [[nodiscard]] std::optional<CBox> boundingBox() override;
    [[nodiscard]] bool                disableSimplification() override;

    [[nodiscard]] const char*         passName() override {
        return "CRiodeGlassBlur";
    }
    [[nodiscard]] ePassElementType type() override {
        return EK_CUSTOM;
    }

  private:
    SData m_data;
};

namespace GlassBlur {
/// Installs the `CRenderPass::add` hook. Returns false when the symbol is
/// missing or already hooked (one plugin may own it).
bool install(HANDLE handle);
/// Removes the hook. Safe to call when not installed.
void uninstall(HANDLE handle);
/// True while the hook is live.
bool active();
/// Change-gated placement log for humans: writes to $RIODE_GLASS_LOG, else
/// /tmp/riode-glass.log. The plugin's own Log::logger copy is unusable (no
/// libhyprland link), so this is a dedicated file.
void logLine(const std::string& line);
/// Runtime blur radius in framebuffer px (IPC-tunable).
void        setRadius(float radius);
float       radius();
/// Regions blurred on the last executed frame (for `riode status`).
int         lastFrameRegions();
/// Total blur draws since load (for `riode status`).
uint64_t    totalDraws();
uint64_t    drawEntry();
uint64_t    glErrors();
uint64_t    exitStage(int stage);
const char* geoSnapshot();
const char* geoLastSnapshot();
uint64_t    addCalls();
uint64_t    surfaceAdds();
uint64_t    dbgNoRes();
uint64_t    dbgNoView();
uint64_t    dbgNoViewWin();
uint64_t    dbgNoViewLayer();
uint64_t    dbgNoViewOther();
uint64_t    dbgNoEffect();
uint64_t    dbgEmptyRegion();
} // namespace GlassBlur
