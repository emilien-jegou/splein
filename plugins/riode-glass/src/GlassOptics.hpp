#pragma once

// Compositor-side liquid-glass optics: GLSL ES 3.00 port of the wgpu
// optics shader (`crates/core/riode/src/foundation/effect/shaders/
// glass/`, one WGSL file per effect, itself a port of
// `adaptive_liquid_glass/shaders/liquid_glass.frag`). Line-for-line
// translation; the WGSL modules document every stage.
//
// Deliberate deltas vs the wgpu twin (all plumbing, none visual):
// - One texture (the core-blurred backdrop): the wide tail, post-lens
//   blur and per-shape extra frost are taps on it.
// - `px` is sample-box-relative (`gl_FragCoord.xy - boxMin`), `uSize`
//   is the box size. Y-down throughout: the compositor's capture rows
//   are used directly as GL rows (mirrored capture), exactly like the
//   copy pass, so no flip.
// - Uniforms ride a std140 UBO (`GlassU`: engine header + `uSize` +
//   the packed uniform stream — the 736-float prefix plus the 56-float
//   glow tail since wire v5), bound per draw by the caller.
//
// No CPU-side optics math remains here: shape, constants, smoothing,
// clip, frost and light all arrive packed, exactly as the reference
// packs them.

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/render/Framebuffer.hpp>

#include <string>

namespace GlassOptics {
// Island-local shape from a hint shape string ("x,y,wxh r1,r2,r3,r4" as
// produced by the plugin status formatter, or "none"). Diagnostics only
// since 8.4 (the optics read shapes from the packed frame); the blur
// pass still logs them per island.
bool parseShape(const std::string& shape, float& x, float& y, float& w, float& h, float radii[4]);

// CSS border-radius clamp (mirrors engine + hyprglass). Diagnostics only
// since 8.4 — see parseShape.
void clampRadii(float radii[4], float w, float h);

// Compiles (once) the optics program. False on failure (notification posted).
bool ensureShader(HANDLE handle);

// Draws one island's optics: fullscreen triangle over the core-blurred
// backdrop, scissored to the island's (padded) union box. The SDF
// discards outside the shapes (transparent, blend-safe). `packed` must
// point at the full uniform stream, stable for the call.
struct DrawArgs {
    SP<Render::IFramebuffer> coreFb;
    // Bound pass target (GL framebuffer id) — the caller leaves DRAW bound
    // to a pre-blur temp; optics must rebind the pass target.
    unsigned int drawFbid = 0;
    // When nonzero, render the lens card HERE instead of `drawFbid`
    // (box-sized, cleared): the caller runs trailing stages on it and
    // composites masked afterwards. Box-relative px starts at the
    // framebuffer origin, so u_boxMin is zeroed for that draw.
    unsigned int offscreenFbid = 0;
    // Sample-box origin + size, capture rows (y-down == GL rows here).
    float sampleMinX = 0.0f, sampleMinY = 0.0f;
    float sampleW = 0.0f, sampleH = 0.0f;
    // Pass target size (viewport restore), GL-px.
    int fbW = 0, fbH = 0;
    // Scissor = island union box, GL-px.
    int scissorX = 0, scissorY = 0, scissorW = 0, scissorH = 0;
    // Frame blur sigmas, pack-space device px (informational; the blur
    // pass already consumed them — the shader ignores this header).
    float sigmaX = 0.0f, sigmaY = 0.0f;
    // Packed uniform stream (prefix + glow tail since wire v5).
    const float* packed = nullptr;
};
void draw(const DrawArgs& args);

uint64_t opticsDraws();
} // namespace GlassOptics
