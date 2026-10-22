// See GlassBlur.hpp for the design.

#include "Format.hpp"
#include "GlassBlur.hpp"
#include "ItemHints.hpp"
#include "ShaderGlue.hpp"

#include <hyprland/src/desktop/view/WLSurface.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/render/OpenGL.hpp>
#include <hyprland/src/render/pass/Pass.hpp>
#include <hyprland/src/render/pass/SurfacePassElement.hpp>
#include <hyprland/src/render/Renderer.hpp>

#include <GLES3/gl32.h>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstring>
#include <cstdio>
#include <ctime>
#include <format>

namespace {

using renderPassAddFn = void (*)(Render::CRenderPass*, UP<IPassElement>&&);
using renderPassRenderFn = CRegion (*)(Render::CRenderPass*, const CRegion&);

inline HANDLE              g_handle      = nullptr;
inline CFunctionHook*       g_hook        = nullptr;
inline CFunctionHook*       g_renderHook  = nullptr;
// Glass bounding boxes recorded at pass-build time (frame-pixel space),
// consumed by the render hook that expands frame damage.
inline std::vector<CBox>   g_renderBoxes;
inline SP<CShader>          g_blurShader;
inline SP<CShader>          g_copyShader;
inline SP<Render::IFramebuffer>       g_sampleFB;
inline SP<Render::IFramebuffer>       g_tempFB;
inline SP<Render::IFramebuffer>       g_coreFB;
// v3 stage chain: two box-sized pings (snapshot never written), the
// ReadBlurred backdrop source, and the offscreen lens card (only while
// post-glass stages exist — without them optics composites direct as
// before).
inline SP<Render::IFramebuffer>       g_fx0;
inline SP<Render::IFramebuffer>       g_fx1;
inline SP<Render::IFramebuffer>       g_bdFB;
inline SP<Render::IFramebuffer>       g_cardFB;
inline GLuint                         g_fxUbo = 0;
inline GLuint                         g_vao  = 0;
inline GLuint                         g_vbo  = 0;
// Plain (no-material) blur radius in px = 3σ of the iOS core blur
// sigma (blurSigma 5.9236 ≈ σ 5.9 → 17.8). Optics islands carry their
// own plateau/rim sigmas and ignore it.
inline float                          g_radius      = 17.8f;
inline int                            g_lastRegions = 0;
inline uint64_t                       g_totalDraws  = 0;
inline uint64_t                       g_drawEntry   = 0;
inline uint64_t                       g_glErrors    = 0;
// draw() early-exit stages: 0 shaders, 1 gl-null, 2 no-fbid, 3 no-monitor,
// 4 bad-size, 5 no-rects.
inline uint64_t                       g_exitStage[6] = {};
// First-draw geometry snapshot.
inline bool                           g_geoTaken      = false;
inline double                         g_geoPos[2]     = {};
inline float                          g_geoScale      = 0.0f;
inline int                            g_geoFb[2]      = {};
inline int                            g_geoNRects     = -1;
inline int                            g_geoFirst[4]   = {};
inline int                            g_geoKept       = 0;
// Last-draw geometry snapshot (inputs actually used for the last blur
// element queued): surface origin, region extents, hint shape.
inline double g_geoLastPos[2] = {};
inline double g_geoLastRegion[4] = {};
inline double g_geoLastShape[4] = {};
inline double g_geoLastRadii[4] = {};
inline int    g_geoLastOptics = -1;
inline uint64_t                       g_addCalls    = 0;
inline uint64_t                       g_surfaceAdds = 0;
// Rejection-stage counters (which check drops the surface element).
inline uint64_t                       g_noRes       = 0;
inline uint64_t                       g_noView      = 0;
inline uint64_t                       g_noEffect    = 0;
inline uint64_t                       g_emptyRegion = 0;
// no_view split: which element kind lacks a CWLSurface view.
inline uint64_t                       g_noViewWin   = 0;
inline uint64_t                       g_noViewLayer = 0;
inline uint64_t                       g_noViewOther = 0;

// Change-gated placement log: emits only when the geometry string changes,
// so a static demo logs once and a moving one logs continuously. Lets a human
// compare the computed blur placement against the client's real screen box.
//
// Writes to $RIODE_GLASS_LOG, else /tmp/riode-glass.log. We cannot use
// Hyprland's Log::logger: this plugin links no libhyprland, so its inline
// `Log::logger` is a private copy that was never handed the log file.
void logPlacementChanged(const std::string& line) {
    static std::string last;
    if (line == last)
        return;
    last = line;
    const char* path = std::getenv("RIODE_GLASS_LOG");
    FILE*       f    = std::fopen(path && *path ? path : "/tmp/riode-glass.log", "a");
    if (!f)
        return;
    // Wall-clock prefix (change-gated lines still land in real time, so a
    // human's hover episodes can be correlated with state flips).
    auto                           now = std::chrono::system_clock::now();
    auto                           ms  = std::chrono::duration_cast<std::chrono::milliseconds>(now.time_since_epoch()) % 1000;
    std::time_t                    t   = std::chrono::system_clock::to_time_t(now);
    std::tm                        tm{};
    localtime_r(&t, &tm);
    std::fprintf(f, "[%02d:%02d:%02d.%03lld] %s\n", tm.tm_hour, tm.tm_min, tm.tm_sec,
                 static_cast<long long>(ms.count()), line.c_str());
    std::fflush(f);
    std::fclose(f);
}

// Own trivial passthrough vertex: positions [0,1] to clip. Attribute
// layout matches Hyprland's fullscreen-quad VAO (0 = pos); the second
// array (uv) is simply never read — the quad is symmetric by
// construction, so even a swapped layout would be harmless.
constexpr const char* BLUR_VERT = R"GLSL(
#version 300 es
layout(location = 0) in vec2 pos;
uniform mat3 proj;
out vec2 v_uv;
void main() {
    v_uv = pos;
    vec3 p = proj * vec3(pos, 1.0);
    gl_Position = vec4(p.xy, 0.0, 1.0);
}
)GLSL";

// Own separable Gaussian: sigma from the radius uniform, linear-sampled
// tap pairs. Same family as the engine's CPU blur; the look is ours.
constexpr const char* BLUR_FRAG = R"GLSL(
#version 300 es
precision highp float;
uniform sampler2D tex;
uniform vec2 direction;
uniform float blurRadius;
in vec2 v_uv;
layout(location = 0) out vec4 fragColor;
void main() {
    float sigma = max(blurRadius / 3.0, 0.001);
    float invSigma2 = -0.5 / (sigma * sigma);
    int samples = min(int(ceil(blurRadius)), 8);
    float w0 = 1.0;
    vec4 result = clamp(texture(tex, v_uv), 0.0, 1.0) * w0;
    float totalWeight = w0;
    for (int i = 1; i <= samples; i += 2) {
        float x1 = float(i);
        float x2 = float(i + 1);
        float w1 = exp(x1 * x1 * invSigma2);
        float w2 = (i + 1 <= samples) ? exp(x2 * x2 * invSigma2) : 0.0;
        float wSum = w1 + w2;
        if (wSum < 0.0001) continue;
        float offset = (x1 * w1 + x2 * w2) / wSum;
        result += clamp(texture(tex, v_uv + direction * offset), 0.0, 1.0) * wSum;
        result += clamp(texture(tex, v_uv - direction * offset), 0.0, 1.0) * wSum;
        totalWeight += 2.0 * wSum;
    }
    fragColor = result / totalWeight;
}
)GLSL";

// Own masked copy-back: gl_FragCoord (GL-px, y-up) mapped into the sample
// texture. Scissor confines each draw to one region rect. Alpha passes
// through: the post-stage card composite relies on the optics' rounded
// silhouette (premultiplied, transparent outside the shape); forcing it
// to 1.0 would paint a full rectangle and square off the corners.
constexpr const char* COPY_FRAG = R"GLSL(
#version 300 es
precision highp float;
uniform sampler2D tex;
uniform vec2 sampleMin;   // sample-box origin, GL-px (y-up)
uniform vec2 invSample;   // 1 / sample-box size
in vec2 v_uv;
layout(location = 0) out vec4 fragColor;
void main() {
    vec2 uv = (gl_FragCoord.xy - sampleMin) * invSample;
    fragColor = texture(tex, uv);
}
)GLSL";

constexpr std::array<float, 9> FULLSCREEN_PROJECTION = {
    2.0f, 0.0f, 0.0f,
    0.0f, 2.0f, 0.0f,
    -1.0f, -1.0f, 1.0f,
};

GLuint fbId(const SP<Render::IFramebuffer>& fb) {
    if (!fb)
        return 0;
    auto* gl = dynamic_cast<Render::GL::CGLFramebuffer*>(fb.get());
    return gl ? gl->getFBID() : 0;
}

// Raw FBO→FBO blit (NEAREST). Coordinates in each framebuffer's own GL-px.
void blitFB(GLuint srcId, GLuint dstId, int sx0, int sy0, int sx1, int sy1) {
    glBindFramebuffer(GL_READ_FRAMEBUFFER, srcId);
    glBindFramebuffer(GL_DRAW_FRAMEBUFFER, dstId);
    const GLint w = sx1 - sx0, h = sy1 - sy0;
    glBlitFramebuffer(sx0, sy0, sx1, sy1, 0, 0, w, h, GL_COLOR_BUFFER_BIT, GL_NEAREST);
}

// Full caller GL state, saved on entry and restored on exit. The element
// runs mid-pass: Hyprland scissors damage/clips, uses stencil in places,
// and sequences draws assuming its own program/VAO/viewport/FB bindings.
// Leaking ANY of it corrupts the rest of the frame (flashes, duplicated
// cursor, overdraw) — the 3a corruption class. Queries only, no stalls.
struct SavedGLState {
    GLint     drawFbid = 0, readFbid = 0;
    GLint     viewport[4] = {};
    GLint     program = 0, vao = 0, activeTex = 0;
    GLint     scissorBox[4] = {};
    GLboolean scissor = GL_FALSE, stencil = GL_FALSE, blend = GL_FALSE;

    void save() {
        glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &drawFbid);
        glGetIntegerv(GL_READ_FRAMEBUFFER_BINDING, &readFbid);
        glGetIntegerv(GL_VIEWPORT, viewport);
        glGetIntegerv(GL_CURRENT_PROGRAM, &program);
        glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &vao);
        glGetIntegerv(GL_ACTIVE_TEXTURE, &activeTex);
        glGetIntegerv(GL_SCISSOR_BOX, scissorBox);
        scissor = glIsEnabled(GL_SCISSOR_TEST);
        stencil = glIsEnabled(GL_STENCIL_TEST);
        blend   = glIsEnabled(GL_BLEND);
    }

    void restore() const {
        glBindFramebuffer(GL_DRAW_FRAMEBUFFER, drawFbid);
        glBindFramebuffer(GL_READ_FRAMEBUFFER, readFbid);
        glViewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        glUseProgram(program);
        glBindVertexArray(vao);
        glActiveTexture(activeTex);
        if (scissor)
            glEnable(GL_SCISSOR_TEST);
        else
            glDisable(GL_SCISSOR_TEST);
        glScissor(scissorBox[0], scissorBox[1], scissorBox[2], scissorBox[3]);
        if (stencil)
            glEnable(GL_STENCIL_TEST);
        else
            glDisable(GL_STENCIL_TEST);
        if (blend)
            glEnable(GL_BLEND);
        else
            glDisable(GL_BLEND);
    }
};

bool ensureShaders() {
    if (g_blurShader && g_copyShader && g_vao)
        return true;
    if (!g_blurShader) {
        g_blurShader = makeShared<CShader>();
        if (!g_blurShader->createProgram(BLUR_VERT, BLUR_FRAG, true)) {
            HyprlandAPI::addNotification(g_handle, "[riode-glass] blur shader failed to compile", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
            g_blurShader.reset();
            return false;
        }
    }
    if (!g_copyShader) {
        g_copyShader = makeShared<CShader>();
        if (!g_copyShader->createProgram(BLUR_VERT, COPY_FRAG, true)) {
            HyprlandAPI::addNotification(g_handle, "[riode-glass] copy shader failed to compile", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
            g_copyShader.reset();
            return false;
        }
    }
    if (!g_vao) {
        // Own fullscreen quad; independent of Hyprland's VAO conventions.
        constexpr float QUAD[16] = {
            0.0f, 0.0f, 0.0f, 0.0f,
            1.0f, 0.0f, 1.0f, 0.0f,
            0.0f, 1.0f, 0.0f, 1.0f,
            1.0f, 1.0f, 1.0f, 1.0f,
        };
        glGenVertexArrays(1, &g_vao);
        glGenBuffers(1, &g_vbo);
        glBindVertexArray(g_vao);
        glBindBuffer(GL_ARRAY_BUFFER, g_vbo);
        glBufferData(GL_ARRAY_BUFFER, sizeof(QUAD), QUAD, GL_STATIC_DRAW);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(float), (void*)0);
        glEnableVertexAttribArray(1);
        glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(float), (void*)(2 * sizeof(float)));
        glBindVertexArray(0);
        glBindBuffer(GL_ARRAY_BUFFER, 0);
    }
    return true;
}

// Monitor-logical y-down box -> framebuffer-px rect. The capture arrives
// mirrored (up was down, down was up), so GL rows already run y-down like
// the screen: no fbH flip here — the whole screen shares one orientation.
struct GLRect {
    int x, y, w, h;
};

GLRect toGL(const CBox& logical, float scale, int /*fbH*/) {
    int x = static_cast<int>(std::floor(logical.x * scale));
    int y = static_cast<int>(std::floor(logical.y * scale));
    int w = static_cast<int>(std::ceil((logical.x + logical.w) * scale)) - x;
    int h = static_cast<int>(std::ceil((logical.y + logical.h) * scale)) - y;
    return {x, y, std::max(w, 0), std::max(h, 0)};
}

void hkRenderPassAdd(Render::CRenderPass* pass, UP<IPassElement>&& el) {
    const auto original = (renderPassAddFn)g_hook->m_original;
    g_addCalls++;
    if (el && el->type() == EK_SURFACE) {
        g_surfaceAdds++;
        auto* se = static_cast<CSurfacePassElement*>(el.get());
        if (!se->m_data.surface) {
            g_noRes++;
        } else {
            // fromResource() only resolves top-level views; island
            // subsurfaces have none. Every resource carries its own view
            // wrapper instead (m_hlSurface) — use it directly.
            auto view = se->m_data.surface->m_hlSurface.lock();
            if (!view) {
                g_noView++;
                if (se->m_data.pWindow)
                    g_noViewWin++;
                else if (se->m_data.pLS)
                    g_noViewLayer++;
                else
                    g_noViewOther++;
            } else if (!view->m_hasBackgroundEffect) {
                g_noEffect++;
            } else if (view->m_blurRegion.empty()) {
                g_emptyRegion++;
            } else {
                // Item hints (material/shape latch on the surface's own
                // precommit/commit via the helper).
                wl_resource* rawSurface = nullptr;
                if (auto wire = se->m_data.surface->getResource())
                    rawSurface = wire->resource();
                if (rawSurface)
                    ItemHints::ensureTracked(rawSurface);
                // Surgical native-blur suppression for exactly this draw:
                // the queued element is a per-draw copy, so no persistent
                // state changes and unload restores native behavior.
                se->m_data.blur = false;
                CGlassBlurElement::SData data;
                g_geoLastPos[0] = se->m_data.pos.x;
                g_geoLastPos[1] = se->m_data.pos.y;
                data.region = view->m_blurRegion;
                data.monitor = se->m_data.pMonitor;
                data.monitorScale = 1.0f;
                data.surfacePos = se->m_data.pos;
                if (auto monitor = se->m_data.pMonitor.lock()) {
                    data.monitorScale = monitor->m_scale;
                    // m_data.pos is GLOBAL; the mask math is monitor-local.
                    data.surfacePos.x -= monitor->m_position.x;
                    data.surfacePos.y -= monitor->m_position.y;
                }
                data.radiusPx = g_radius;
                // Optics when the island carries a material hint; the
                // shape falls back to the region extents (square) when the
                // hint has none. No material → Gaussian path. The hint is
                // re-read every frame the surface draws, so moves and
                // material edits apply on the next commit without the
                // client doing anything special.
                if (rawSurface) {
                    ItemHints::ItemInfo info;
                    if (ItemHints::get(rawSurface, info)) {
                        data.effects = info.effects;
                        data.glassIndex = info.glass_stack_index;
                    }
                    if (info.has_packed) {
                        data.useOptics = true;
                        data.packSigmaX = info.pack_sigma_x;
                        data.packSigmaY = info.pack_sigma_y;
                        data.packBoxX = info.pack_box_x;
                        data.packBoxY = info.pack_box_y;
                        data.packBoxW = info.pack_box_w;
                        data.packBoxH = info.pack_box_h;
                        static_assert(sizeof(data.packed) == sizeof(info.packed),
                                      "packed frame size must match the item hint");
                        memcpy(data.packed, info.packed, sizeof(data.packed));
                        g_geoLastOptics = 1;
                        float sx, sy, sw, sh, radii[4];
                        if (GlassOptics::parseShape(info.shape, sx, sy, sw, sh, radii)) {
                            data.shapeLocal = CBox{(double)sx, (double)sy, (double)sw, (double)sh};
                            for (int i = 0; i < 4; ++i)
                                data.shapeRadii[i] = radii[i];
                        } else {
                            // Region extents, square corners.
                            double x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9;
                            for (const auto& r : data.region.getRects()) {
                                x0 = std::min(x0, (double)r.x1);
                                y0 = std::min(y0, (double)r.y1);
                                x1 = std::max(x1, (double)r.x2);
                                y1 = std::max(y1, (double)r.y2);
                            }
                            if (x1 > x0 && y1 > y0)
                                data.shapeLocal = CBox{x0, y0, x1 - x0, y1 - y0};
                        }
                        GlassOptics::clampRadii(data.shapeRadii, (float)data.shapeLocal.w, (float)data.shapeLocal.h);
                    } else {
                        g_geoLastOptics = 0;
                    }
                } else {
                    g_geoLastOptics = -1;
                }
                {
                    const auto rects = data.region.getRects();
                    if (!rects.empty()) {
                        g_geoLastRegion[0] = rects[0].x1;
                        g_geoLastRegion[1] = rects[0].y1;
                        g_geoLastRegion[2] = rects[0].x2;
                        g_geoLastRegion[3] = rects[0].y2;
                    }
                    g_geoLastShape[0] = data.shapeLocal.x;
                    g_geoLastShape[1] = data.shapeLocal.y;
                    g_geoLastShape[2] = data.shapeLocal.x + data.shapeLocal.w;
                    g_geoLastShape[3] = data.shapeLocal.y + data.shapeLocal.h;
                    for (int i = 0; i < 4; ++i)
                        g_geoLastRadii[i] = data.shapeRadii[i];
                }
                {
                    // Placement log: region/shape in monitor-local y-down screen
                    // coords, directly comparable to `hyprctl clients`.
                    const auto regionRects = data.region.getRects();
                    double     rx0 = 1e9, ry0 = 1e9, rx1 = -1e9, ry1 = -1e9;
                    for (const auto& r : regionRects) {
                        rx0 = std::min(rx0, data.surfacePos.x + r.x1);
                        ry0 = std::min(ry0, data.surfacePos.y + r.y1);
                        rx1 = std::max(rx1, data.surfacePos.x + r.x2);
                        ry1 = std::max(ry1, data.surfacePos.y + r.y2);
                    }
                    const auto mon = se->m_data.pMonitor.lock();
                    if (mon) {
                        const bool haveRect = !regionRects.empty();
                        logPlacementChanged(rfmt::format(
                            "[riode-glass] place: mon=({:.0f},{:.0f} {:.0f}x{:.0f}) scale={:.2f} global=({:.0f},{:.0f}) "
                            "surfLocal=({:.0f},{:.0f}) nrects={} regionLocal=[{:.0f},{:.0f},{:.0f},{:.0f}] "
                            "regionScreen=[{:.0f},{:.0f},{:.0f},{:.0f}] shapeScreen=[{:.0f},{:.0f},{:.0f},{:.0f}] optics={}",
                            mon->m_position.x, mon->m_position.y, mon->m_transformedSize.x, mon->m_transformedSize.y, mon->m_scale,
                            se->m_data.pos.x, se->m_data.pos.y, data.surfacePos.x, data.surfacePos.y,
                            static_cast<int>(regionRects.size()),
                            haveRect ? regionRects[0].x1 : 0.0, haveRect ? regionRects[0].y1 : 0.0,
                            haveRect ? regionRects[0].x2 : 0.0, haveRect ? regionRects[0].y2 : 0.0,
                            rx0, ry0, rx1, ry1,
                            data.shapeLocal.x + data.surfacePos.x, data.shapeLocal.y + data.surfacePos.y,
                            data.shapeLocal.x + data.surfacePos.x + data.shapeLocal.w,
                            data.shapeLocal.y + data.surfacePos.y + data.shapeLocal.h, data.useOptics ? 1 : 0));
                    }
                }
                auto* glassEl = new CGlassBlurElement(data);
                if (auto bb = glassEl->boundingBox())
                    g_renderBoxes.push_back(bb->scale(data.monitorScale));
                original(pass, UP<IPassElement>(glassEl));
                g_lastRegions += static_cast<int>(data.region.getRects().size());
            }
        }
    }
    original(pass, std::move(el));
}

// Complete our needsLiveBlur contract. Hyprland expands frame damage by
// (bbox ∩ damage) ⊕ blur-radius only, so a cursor-sized frame refreshes
// just the blob inside our box — the rest of our capture then reads
// persisted previous-frame pixels (our own lens output feeding back; the
// midtone-lift probe signatures). Re-expand the incoming damage by every
// recorded glass box so the backdrop below us re-renders fresh across
// our whole sample whenever any damage exists. Boxes are recorded during
// the build above (rebuilds every frame) and consumed here.
CRegion hkRenderPassRender(Render::CRenderPass* pass, const CRegion& damage) {
    const auto original = (renderPassRenderFn)g_renderHook->m_original;
    CRegion    expanded = damage;
    if (!damage.empty()) {
        for (const auto& b : g_renderBoxes)
            expanded.add(b);
    }
    auto out = original(pass, expanded);
    g_renderBoxes.clear();
    return out;
}

} // namespace

CGlassBlurElement::CGlassBlurElement(SData data) : m_data(std::move(data)) {}

bool CGlassBlurElement::needsLiveBlur() {
    // The pass must fully re-render the backdrop below us before we
    // sample it; without this, partial damage mixes fresh wallpaper with
    // our own stale output.
    return true;
}

bool CGlassBlurElement::needsPrecomputeBlur() {
    return false;
}

std::optional<CBox> CGlassBlurElement::boundingBox() {
    // Region bounds in monitor-local logical coords (+ blur reach, so the
    // damage system keeps our whole kernel input fresh). Must agree with
    // needsLiveBlur: a live-blur element without a box aborts the
    // compositor.
    CBox box{1e9, 1e9, -1e9, -1e9};
    bool any = false;
    for (const auto& r : m_data.region.getRects()) {
        const double x0 = m_data.surfacePos.x + r.x1;
        const double y0 = m_data.surfacePos.y + r.y1;
        const double x1 = m_data.surfacePos.x + r.x2;
        const double y1 = m_data.surfacePos.y + r.y2;
        box.x      = std::min(box.x, x0);
        box.y      = std::min(box.y, y0);
        box.w      = std::max(box.w, x1 - box.x);
        box.h      = std::max(box.h, y1 - box.y);
        any        = true;
    }
    if (!any)
        return std::nullopt;
    // Recompute w/h from final min/max (the running update above skews
    // once x/y move after w/h were set).
    double x1 = box.x, y1 = box.y;
    for (const auto& r : m_data.region.getRects()) {
        x1 = std::max(x1, m_data.surfacePos.x + r.x2);
        y1 = std::max(y1, m_data.surfacePos.y + r.y2);
    }
    box.w = x1 - box.x;
    box.h = y1 - box.y;
    const double pad = (m_data.radiusPx * 2.0 + 4.0) / m_data.monitorScale;
    box.x -= pad;
    box.y -= pad;
    box.w += 2.0 * pad;
    box.h += 2.0 * pad;
    return box;
}

bool CGlassBlurElement::disableSimplification() {
    // Always run when the pass runs: a discarded blur with a drawn surface
    // would show unblurred content over a stale frame.
    return true;
}

void runSeparable(const SP<Render::IFramebuffer>& src,
                  const SP<Render::IFramebuffer>& dst,
                  float radiusXPx,
                  float radiusYPx,
                  int w,
                  int h) {
    auto* gl = Render::GL::g_pHyprOpenGL.get();
    if (!gl || !src || !dst || !g_tempFB)
        return;
    auto srcTex = src->getTexture();
    auto dstTex = dst->getTexture();
    auto tmpTex = g_tempFB->getTexture();
    if (!srcTex || !dstTex || !tmpTex)
        return;
    const GLuint srcId = fbId(src), dstId = fbId(dst), tmpId = fbId(g_tempFB);
    if (!srcId || !dstId || !tmpId)
        return;
    blitFB(srcId, dstId, 0, 0, w, h);
    const float radiusMax = std::max(radiusXPx, radiusYPx);
    const int iterations = std::clamp(static_cast<int>(std::round(radiusMax / 8.0f)), 1, 3);
    auto      shader     = gl->useShader(g_blurShader).lock();
    if (!shader)
        return;
    const auto program = shader->program();
    shader->setUniformMatrix3fv(SHADER_PROJ, 1, GL_FALSE, FULLSCREEN_PROJECTION);
    shader->setUniformInt(SHADER_TEX, 0);
    const GLint dirLoc = glGetUniformLocation(program, "direction");
    const GLint radLoc = glGetUniformLocation(program, "blurRadius");
    glActiveTexture(GL_TEXTURE0);
    glBindVertexArray(g_vao);
    glViewport(0, 0, w, h);
    for (int i = 0; i < iterations; ++i) {
        // H pass: current result -> temp.
        glBindFramebuffer(GL_DRAW_FRAMEBUFFER, tmpId);
        dstTex->bind();
        glUniform1f(radLoc, radiusXPx);
        glUniform2f(dirLoc, 1.0f / w, 0.0f);
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        // V pass: temp -> dst. Result is in dst after every iteration.
        glBindFramebuffer(GL_DRAW_FRAMEBUFFER, dstId);
        tmpTex->bind();
        glUniform1f(radLoc, radiusYPx);
        glUniform2f(dirLoc, 0.0f, 1.0f / h);
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
    }
    glBindVertexArray(0);
}

std::vector<UP<IPassElement>> CGlassBlurElement::draw() {
    g_drawEntry++;
    SavedGLState saved;
    saved.save();
    auto restore = [&] {
        saved.restore();
    };
    // Drain pre-existing errors so the post-pass drain only counts ours.
    while (glGetError() != GL_NO_ERROR) {}
    if (!ensureShaders()) {
        g_exitStage[0]++;
        restore();
        return {};
    }
    auto* gl = Render::GL::g_pHyprOpenGL.get();
    if (!gl) {
        g_exitStage[1]++;
        restore();
        return {};
    }

    // 0.56 has no currentFB accessor: the bound DRAW framebuffer IS the
    // pass target. Size comes from the monitor (transformed = FB native).
    GLint drawFbid = 0;
    glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &drawFbid);
    if (!drawFbid) {
        g_exitStage[2]++;
        restore();
        return {};
    }
    auto monitor = m_data.monitor.lock();
    if (!monitor) {
        g_exitStage[3]++;
        restore();
        return {};
    }
    const int fbW = static_cast<int>(monitor->m_transformedSize.x);
    const int fbH = static_cast<int>(monitor->m_transformedSize.y);
    if (fbW <= 0 || fbH <= 0) {
        g_exitStage[4]++;
        restore();
        return {};
    }

    const float scale  = m_data.monitorScale;
    const float radius = std::clamp(m_data.radiusPx, 1.0f, 64.0f);
    // Gaussian kernel reach for plain-blur islands. Optics islands use
    // the wire header's integer sample box verbatim (no recomputation,
    // no rounding drift) — see the override after the rect loop.
    int pad = static_cast<int>(std::ceil(radius)) + 2;

    // Union of region rects, padded for the kernel, in FB-px y-down.
    int ux0 = fbW, uy0 = fbH, ux1 = 0, uy1 = 0;
    std::vector<CBox> rects;
    const auto regionRects = m_data.region.getRects();
    if (!g_geoTaken) {
        g_geoTaken      = true;
        g_geoPos[0]     = m_data.surfacePos.x;
        g_geoPos[1]     = m_data.surfacePos.y;
        g_geoScale      = scale;
        g_geoFb[0]      = fbW;
        g_geoFb[1]      = fbH;
        g_geoNRects     = static_cast<int>(regionRects.size());
        if (!regionRects.empty()) {
            g_geoFirst[0] = regionRects[0].x1;
            g_geoFirst[1] = regionRects[0].y1;
            g_geoFirst[2] = regionRects[0].x2;
            g_geoFirst[3] = regionRects[0].y2;
        }
    }
    for (const auto& r : regionRects) {
        const int x0 = static_cast<int>(std::floor((m_data.surfacePos.x + r.x1) * scale));
        const int y0 = static_cast<int>(std::floor((m_data.surfacePos.y + r.y1) * scale));
        const int x1 = static_cast<int>(std::ceil((m_data.surfacePos.x + r.x2) * scale));
        const int y1 = static_cast<int>(std::ceil((m_data.surfacePos.y + r.y2) * scale));
        if (x1 <= 0 || y1 <= 0 || x0 >= fbW || y0 >= fbH)
            continue;
        rects.push_back(CBox{static_cast<double>(x0), static_cast<double>(y0), static_cast<double>(x1 - x0), static_cast<double>(y1 - y0)});
        ux0 = std::min(ux0, x0 - pad);
        uy0 = std::min(uy0, y0 - pad);
        ux1 = std::max(ux1, x1 + pad);
        uy1 = std::max(uy1, y1 + pad);
    }
    if (rects.empty()) {
        g_exitStage[5]++;
        restore();
        return {};
    }
    g_geoKept = static_cast<int>(rects.size());
    if (m_data.useOptics) {
        // Header box + surface origin: the pack is surface-local
        // (parent mode) or island-local (island layers, rebased to the
        // island origin at upload), so the monitor-FB box is
        // surfacePos·scale + box. Shapes are box-relative, so this is
        // the only placement that keeps px = fragCoord − boxMin in the
        // pack's space. Falls back to the recomputed union when the
        // header box is degenerate (foreign/old payload).
        int bx0 = static_cast<int>(
            std::round(m_data.surfacePos.x * scale + m_data.packBoxX));
        int by0 = static_cast<int>(
            std::round(m_data.surfacePos.y * scale + m_data.packBoxY));
        int bx1 = bx0 + static_cast<int>(std::round(m_data.packBoxW));
        int by1 = by0 + static_cast<int>(std::round(m_data.packBoxH));
        bx0 = std::clamp(bx0, 0, fbW);
        by0 = std::clamp(by0, 0, fbH);
        bx1 = std::clamp(bx1, 0, fbW);
        by1 = std::clamp(by1, 0, fbH);
        if (bx1 > bx0 && by1 > by0) {
            ux0 = bx0;
            uy0 = by0;
            ux1 = bx1;
            uy1 = by1;
        } else {
            ux0 = std::clamp(ux0, 0, fbW);
            uy0 = std::clamp(uy0, 0, fbH);
            ux1 = std::clamp(ux1, 0, fbW);
            uy1 = std::clamp(uy1, 0, fbH);
        }
    } else {
        ux0 = std::clamp(ux0, 0, fbW);
        uy0 = std::clamp(uy0, 0, fbH);
        ux1 = std::clamp(ux1, 0, fbW);
        uy1 = std::clamp(uy1, 0, fbH);
    }
    if (ux1 <= ux0 || uy1 <= uy0)
        return {};
    // Where the blit/scissor will actually land: FB px y-down box. The
    // capture is mirrored (up was down), so these y-down values are used
    // directly as GL rows — one whole-screen orientation, no per-box flip.
    //
    // Entry scissor state (change-gated with the rest): the capture blit
    // below runs while Hyprland's damage clip may still be active (it is
    // only dropped at 708), so a small box here during pointer-driven
    // frames means the sample can be drawn under a partial clip.
    GLint scissorBox[4] = {};
    const GLboolean scissorOn = glIsEnabled(GL_SCISSOR_TEST);
    glGetIntegerv(GL_SCISSOR_BOX, scissorBox);
    GLint viewport[4] = {};
    glGetIntegerv(GL_VIEWPORT, viewport);
    const GLboolean stencilOn = glIsEnabled(GL_STENCIL_TEST);
    GLint stencilFunc = 0, stencilRef = 0, stencilMask = 0;
    glGetIntegerv(GL_STENCIL_FUNC, &stencilFunc);
    glGetIntegerv(GL_STENCIL_REF, &stencilRef);
    glGetIntegerv(GL_STENCIL_VALUE_MASK, &stencilMask);
    logPlacementChanged(rfmt::format(
        "[riode-glass] draw: fb={}x{} scale={:.2f} surfLocal=({:.0f},{:.0f}) unionFBpx=[{},{},{},{}] "
        "glY=[{},{}] glX=[{},{}] scis={} [{},{},{},{}] vp=[{},{},{},{}] sten={} func={} ref={} mask={}",
        fbW, fbH, scale, m_data.surfacePos.x, m_data.surfacePos.y, ux0, uy0, ux1, uy1, uy0, uy1, ux0, ux1,
        scissorOn ? 1 : 0, scissorBox[0], scissorBox[1], scissorBox[2], scissorBox[3], viewport[0], viewport[1],
        viewport[2], viewport[3], stencilOn ? 1 : 0, stencilFunc, stencilRef, stencilMask));

    const int sw = ux1 - ux0, sh = uy1 - uy0;

    if (!g_sampleFB)
        g_sampleFB = g_pHyprRenderer->createFB("riode-glass-sample");
    if (!g_tempFB)
        g_tempFB = g_pHyprRenderer->createFB("riode-glass-temp");
    if (!g_coreFB)
        g_coreFB = g_pHyprRenderer->createFB("riode-glass-core");
    if (!g_sampleFB || !g_tempFB || !g_coreFB) {
        g_exitStage[0]++;
        restore();
        return {};
    }
    // v3 latched stack: sort by absolute index; the glass position
    // splits it into the backdrop walk (before glass: transform the
    // capture) and the card walk (after glass: transform the lens card).
    // No pack (or index -1) => every stage is a backdrop stage.
    std::vector<ItemHints::ItemInfo::EffectStage> stages = m_data.effects;
    std::stable_sort(stages.begin(), stages.end(),
                     [](const auto& a, const auto& b) { return a.stack_index < b.stack_index; });
    const bool    hasGlass = m_data.useOptics;
    const int32_t glassAt  = hasGlass ? m_data.glassIndex : -1;
    std::vector<size_t> preStages, postStages;
    for (size_t i = 0; i < stages.size(); ++i) {
        if (glassAt < 0 || stages[i].stack_index < glassAt)
            preStages.push_back(i);
        else if (stages[i].stack_index > glassAt)
            postStages.push_back(i);
        // stack_index == glassAt: a custom stage on the pack's own slot —
        // the glass owns it; skip (defensive; client sends distinct slots).
    }

    bool allocOk = true;
    auto ensureBox = [&](const SP<Render::IFramebuffer>& fb) {
        if (fb->m_size.x != sw || fb->m_size.y != sh)
            allocOk = fb->alloc(sw, sh, DRM_FORMAT_ARGB8888) && allocOk;
    };
    ensureBox(g_sampleFB);
    ensureBox(g_tempFB);
    ensureBox(g_coreFB);
    if (!stages.empty()) {
        if (!g_fx0)
            g_fx0 = g_pHyprRenderer->createFB("riode-glass-fx0");
        if (!g_fx1)
            g_fx1 = g_pHyprRenderer->createFB("riode-glass-fx1");
        if (g_fx0)
            ensureBox(g_fx0);
        if (g_fx1)
            ensureBox(g_fx1);
        const bool needBlur = std::any_of(stages.begin(), stages.end(), [](const auto& s) {
            return s.backdrop_need == 2u && s.backdrop_sigma > 0.0f;
        });
        if (needBlur) {
            if (!g_bdFB)
                g_bdFB = g_pHyprRenderer->createFB("riode-glass-bd");
            if (g_bdFB)
                ensureBox(g_bdFB);
        }
    }
    if (hasGlass && !postStages.empty()) {
        if (!g_cardFB)
            g_cardFB = g_pHyprRenderer->createFB("riode-glass-card");
        if (g_cardFB)
            ensureBox(g_cardFB);
    }
    if (!allocOk || !g_sampleFB->getTexture()) {
        g_exitStage[0]++;
        restore();
        return {};
    }

    // Sample: blit the union box into the sample FB. Whole-screen y-down
    // rows on both sides (mirrored capture): no flip anywhere, so every
    // consumer (optics + gauss) shares the orientation.
    glBindFramebuffer(GL_READ_FRAMEBUFFER, drawFbid);
    glBindFramebuffer(GL_DRAW_FRAMEBUFFER, fbId(g_sampleFB));
    glBlitFramebuffer(ux0, uy0, ux1, uy1, 0, 0, sw, sh, GL_COLOR_BUFFER_BIT, GL_NEAREST);

    // Ping-pong separable Gaussian at sample resolution.
    gl->blend(false);
    if (glIsEnabled(GL_SCISSOR_TEST))
        glDisable(GL_SCISSOR_TEST);
    if (glIsEnabled(GL_STENCIL_TEST))
        glDisable(GL_STENCIL_TEST);

    // Masked composite helper: one scissored fullscreen draw per region
    // rect from the given source FB. All math in capture (y-down) rows;
    // the sample texture shares the orientation — no flip here.
    auto compositeMasked = [&](const SP<Render::IFramebuffer>& srcFb) -> bool {
        auto shader = gl->useShader(g_copyShader).lock();
        if (!shader) {
            restore();
            return false;
        }
        const auto program = shader->program();
        shader->setUniformMatrix3fv(SHADER_PROJ, 1, GL_FALSE, FULLSCREEN_PROJECTION);
        shader->setUniformInt(SHADER_TEX, 0);
        const GLint sampleMinLoc = glGetUniformLocation(program, "sampleMin");
        const GLint invSampleLoc = glGetUniformLocation(program, "invSample");
        glActiveTexture(GL_TEXTURE0);
        srcFb->getTexture()->bind();
        glBindVertexArray(g_vao);
        gl->blend(true);
        glBindFramebuffer(GL_DRAW_FRAMEBUFFER, drawFbid);
        glViewport(0, 0, fbW, fbH);
        glEnable(GL_SCISSOR_TEST);
        // Sample-box origin in capture (y-down) rows.
        const float sampleMinX = static_cast<float>(ux0);
        const float sampleMinY = static_cast<float>(uy0);
        glUniform2f(invSampleLoc, 1.0f / sw, 1.0f / sh);
        for (const auto& r : rects) {
            const GLRect g = toGL(r, 1.0f, fbH); // rects already in FB px
            if (g.w <= 0 || g.h <= 0)
                continue;
            glScissor(g.x, g.y, g.w, g.h);
            glUniform2f(sampleMinLoc, sampleMinX, sampleMinY);
            glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        }
        glDisable(GL_SCISSOR_TEST);
        glBindVertexArray(0);
        return true;
    };

    // One v3 custom stage: input (previous image) on unit 0, the
    // backdrop on 1 (the raw capture; ReadBlurred swaps in its Gaussian
    // at the stage sigma), the raw capture as the sharp parent on 2,
    // the fixed 8-vec4 uniform block on buffer slot 0. A failed
    // program (transpile/compile/link/reflection, logged once per hash
    // by the glue) makes the stage a no-op — the chain continues; that
    // is the no-fallback contract: unsupported = no effect, loudly.
    if (!g_fxUbo) {
        glGenBuffers(1, &g_fxUbo);
        glBindBuffer(GL_UNIFORM_BUFFER, g_fxUbo);
        glBufferData(GL_UNIFORM_BUFFER, 32 * sizeof(float), nullptr, GL_STREAM_DRAW);
        glBindBuffer(GL_UNIFORM_BUFFER, 0);
    }
    float uData[32] = {};
    auto runStage = [&](const ItemHints::ItemInfo::EffectStage& st, const SP<Render::IFramebuffer>& src,
                        const SP<Render::IFramebuffer>& dst) -> bool {
        if (!fbId(dst) || !src || !src->getTexture())
            return false;
        const auto sp = ShaderGlue::programFor(st.wgsl);
        if (!sp.shader)
            return false;
        // Backdrop source: raw capture (none/sharp; sigma-0 read_blurred
        // is an identity blur), else the frame blur at the stage sigma.
        SP<Render::IFramebuffer> backdropFb = g_sampleFB;
        if (st.backdrop_need == 2u && st.backdrop_sigma > 0.0f && g_bdFB && fbId(g_bdFB)) {
            runSeparable(g_sampleFB, g_bdFB, 3.0f * st.backdrop_sigma, 3.0f * st.backdrop_sigma, sw, sh);
            backdropFb = g_bdFB;
        }
        auto shader = gl->useShader(sp.shader).lock();
        if (!shader)
            return false;
        shader->setUniformMatrix3fv(SHADER_PROJ, 1, GL_FALSE, FULLSCREEN_PROJECTION);
        glActiveTexture(GL_TEXTURE0);
        src->getTexture()->bind();
        glActiveTexture(GL_TEXTURE1);
        backdropFb->getTexture()->bind();
        glActiveTexture(GL_TEXTURE2);
        g_sampleFB->getTexture()->bind();
        // Exactly 128 bytes (std140 array<vec4,8>): shorter latched
        // streams zero-pad, longer ones cannot exist (helper caps).
        std::fill(uData, uData + 32, 0.0f);
        const size_t n = std::min<size_t>(st.uniforms.size(), 32);
        std::copy_n(st.uniforms.begin(), n, uData);
        glBindBuffer(GL_UNIFORM_BUFFER, g_fxUbo);
        glBufferSubData(GL_UNIFORM_BUFFER, 0, sizeof(uData), uData);
        glBindBufferBase(GL_UNIFORM_BUFFER, 0, g_fxUbo);
        glBindBuffer(GL_UNIFORM_BUFFER, 0);
        glBindFramebuffer(GL_DRAW_FRAMEBUFFER, fbId(dst));
        glViewport(0, 0, sw, sh);
        glBindVertexArray(g_vao);
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        glBindVertexArray(0);
        return true;
    };

    // ── v3 pre-glass backdrop walk ────────────────────────────────────────
    // Mirrors the wgpu backdrop walk: the capture (snapshot) is never
    // written; every stage reads the current image and writes the other
    // ping; glass then blurs the walk's current image (below).
    SP<Render::IFramebuffer> chain = g_sampleFB;
    auto nextPing = [&](const SP<Render::IFramebuffer>& cur) {
        return cur.get() == g_fx0.get() ? g_fx1 : g_fx0;
    };
    for (const size_t i : preStages) {
        auto dst = nextPing(chain);
        if (dst && fbId(dst) && runStage(stages[i], chain, dst))
            chain = dst;
    }

    if (m_data.useOptics) {
        // v5 packed island: single anisotropic core blur at the wire
        // σx/σy (the reference's composed blur — one blur per group at
        // the smallest drawn sigma), then the optics pass over it.
        if (!GlassOptics::ensureShader(g_handle)) {
            g_exitStage[0]++;
            restore();
            return {};
        }
        const float sx = std::max(m_data.packSigmaX, 0.0f);
        const float sy = std::max(m_data.packSigmaY, 0.0f);
        // The glass core blur reads the walk's CURRENT image (wgpu:
        // glass blurs the backdrop after the pre-glass stages; the raw
        // snapshot stays the sharp source via the blend target).
        if (std::max(sx, sy) > 0.05f) {
            runSeparable(chain, g_coreFB, 3.0f * sx, 3.0f * sy, sw, sh);
        } else {
            blitFB(fbId(chain), fbId(g_coreFB), 0, 0, sw, sh);
        }
        GlassOptics::DrawArgs args;
        args.coreFb    = g_coreFB;
        args.drawFbid  = (unsigned int)drawFbid;
        args.sampleMinX = static_cast<float>(ux0);
        args.sampleMinY = static_cast<float>(uy0);
        args.sampleW   = static_cast<float>(sw);
        args.sampleH   = static_cast<float>(sh);
        args.fbW = fbW;
        args.fbH = fbH;
        args.scissorX = ux0;
        args.scissorY = uy0;
        args.scissorW = sw;
        args.scissorH = sh;
        args.sigmaX = sx;
        args.sigmaY = sy;
        args.packed = m_data.packed;
        if (!args.coreFb->getTexture()) {
            g_exitStage[0]++;
            restore();
            return {};
        }
        if (postStages.empty() || !g_cardFB || !fbId(g_cardFB)) {
            // No trailing stages (or no card FB): optics composites
            // straight into the pass target, exactly as before.
            GlassOptics::draw(args);
        } else {
            // Trailing stages transform the lens card: render it to an
            // offscreen box-sized FB (cleared — card over transparent),
            // run the chain, composite the result masked like any
            // sample. Same final math: card srcOver onto the real
            // backdrop below.
            GLfloat prevClear[4];
            glGetFloatv(GL_COLOR_CLEAR_VALUE, prevClear);
            glBindFramebuffer(GL_DRAW_FRAMEBUFFER, fbId(g_cardFB));
            glClearColor(0.0f, 0.0f, 0.0f, 0.0f);
            glClear(GL_COLOR_BUFFER_BIT);
            glClearColor(prevClear[0], prevClear[1], prevClear[2], prevClear[3]);
            args.offscreenFbid = fbId(g_cardFB);
            GlassOptics::draw(args);
            SP<Render::IFramebuffer> cardChain = g_cardFB;
            for (const size_t i : postStages) {
                auto dst = nextPing(cardChain);
                if (dst && fbId(dst) && runStage(stages[i], cardChain, dst))
                    cardChain = dst;
            }
            if (!compositeMasked(cardChain))
                return {};
        }
    } else {
        if (stages.empty()) {
            runSeparable(g_sampleFB, g_sampleFB, radius, radius, sw, sh);
            if (!compositeMasked(g_sampleFB))
                return {};
        } else {
            // Custom stages without a glass pack: chain over the capture,
            // then the legacy radius blur, then the masked composite.
            // The ping (never the snapshot) takes the blur output.
            auto dst = nextPing(chain);
            if (!dst || !fbId(dst)) {
                restore();
                return {};
            }
            runSeparable(chain, dst, radius, radius, sw, sh);
            if (!compositeMasked(dst))
                return {};
        }
    }

    // Restore the caller's frame state exactly (see SavedGLState).
    restore();
    while (glGetError() != GL_NO_ERROR)
        g_glErrors++;

    g_totalDraws++;
    return {};
}

namespace GlassBlur {

// Public wrapper so main.cpp can emit load/exit lines through the same
// change-gated file log.
void logLine(const std::string& line) {
    logPlacementChanged(line);
}

bool install(HANDLE handle) {
    g_handle = handle;
    auto matches = HyprlandAPI::findFunctionsByName(handle, "CRenderPass3add");
    void* address = nullptr;
    for (const auto& m : matches) {
        if (m.demangled.contains("CRenderPass::add") && m.demangled.contains("IPassElement")) {
            address = m.address;
            break;
        }
    }
    if (!address) {
        HyprlandAPI::addNotification(handle, "[riode-glass] CRenderPass::add not found — blur hook disabled", CHyprColor{1.0, 0.8, 0.2, 1.0},
                                     10000);
        return false;
    }
    g_hook = HyprlandAPI::createFunctionHook(handle, address, (void*)hkRenderPassAdd);
    if (!g_hook || !g_hook->hook()) {
        if (g_hook) {
            HyprlandAPI::removeFunctionHook(handle, g_hook);
            g_hook = nullptr;
        }
        HyprlandAPI::addNotification(handle, "[riode-glass] CRenderPass::add hook failed (already owned?) — blur disabled",
                                     CHyprColor{1.0, 0.8, 0.2, 1.0}, 10000);
        return false;
    }

    auto rmatches = HyprlandAPI::findFunctionsByName(handle, "CRenderPass6render");
    void* raddress = nullptr;
    for (const auto& m : rmatches) {
        if (m.demangled.contains("CRenderPass::render") && m.demangled.contains("CRegion")) {
            raddress = m.address;
            break;
        }
    }
    if (!raddress) {
        HyprlandAPI::addNotification(handle, "[riode-glass] CRenderPass::render not found — hover artifact fix disabled",
                                     CHyprColor{1.0, 0.8, 0.2, 1.0}, 10000);
        return false;
    }
    g_renderHook = HyprlandAPI::createFunctionHook(handle, raddress, (void*)hkRenderPassRender);
    if (!g_renderHook || !g_renderHook->hook()) {
        if (g_renderHook) {
            HyprlandAPI::removeFunctionHook(handle, g_renderHook);
            g_renderHook = nullptr;
        }
        HyprlandAPI::addNotification(handle, "[riode-glass] CRenderPass::render hook failed — hover artifact fix disabled",
                                     CHyprColor{1.0, 0.8, 0.2, 1.0}, 10000);
        return false;
    }
    return true;
}

void uninstall(HANDLE handle) {
    if (g_renderHook) {
        HyprlandAPI::removeFunctionHook(handle, g_renderHook);
        g_renderHook = nullptr;
    }
    if (g_hook) {
        HyprlandAPI::removeFunctionHook(handle, g_hook);
        g_hook = nullptr;
    }
    g_blurShader.reset();
    g_copyShader.reset();
    g_sampleFB.reset();
    g_tempFB.reset();
    if (g_vao) {
        glDeleteVertexArrays(1, &g_vao);
        glDeleteBuffers(1, &g_vbo);
        g_vao = g_vbo = 0;
    }
    g_handle = nullptr;
}

bool     active() {
    return g_hook != nullptr;
}
void     setRadius(float radius) {
    g_radius = radius;
}
float    radius() {
    return g_radius;
}
int      lastFrameRegions() {
    return std::exchange(g_lastRegions, 0);
}
uint64_t    totalDraws() {
    return g_totalDraws;
}
uint64_t    drawEntry() {
    return g_drawEntry;
}
uint64_t    glErrors() {
    return g_glErrors;
}
uint64_t    exitStage(int stage) {
    if (stage < 0 || stage > 5)
        return 0;
    return g_exitStage[stage];
}
const char* geoSnapshot() {
    static std::string cache;
    cache = rfmt::format("pos=({:.1f},{:.1f}) scale={:.2f} fb={}x{} nrects={} first=[{},{},{},{}] kept={}", g_geoPos[0],
                        g_geoPos[1], g_geoScale, g_geoFb[0], g_geoFb[1], g_geoNRects, g_geoFirst[0], g_geoFirst[1],
                        g_geoFirst[2], g_geoFirst[3], g_geoKept);
    return cache.c_str();
}
const char* geoLastSnapshot() {
    static std::string cache;
    cache = rfmt::format("pos=({:.1f},{:.1f}) region=[{:.0f},{:.0f},{:.0f},{:.0f}] shape=[{:.0f},{:.0f},{:.0f},{:.0f}] radii=[{:.0f},{:.0f},{:.0f},{:.0f}] optics={}",
                        g_geoLastPos[0], g_geoLastPos[1], g_geoLastRegion[0], g_geoLastRegion[1], g_geoLastRegion[2],
                        g_geoLastRegion[3], g_geoLastShape[0], g_geoLastShape[1], g_geoLastShape[2], g_geoLastShape[3],
                        g_geoLastRadii[0], g_geoLastRadii[1], g_geoLastRadii[2], g_geoLastRadii[3], g_geoLastOptics);
    return cache.c_str();
}
uint64_t    addCalls() {
    return g_addCalls;
}
uint64_t    surfaceAdds() {
    return g_surfaceAdds;
}
uint64_t    dbgNoRes() {
    return g_noRes;
}
uint64_t    dbgNoView() {
    return g_noView;
}
uint64_t    dbgNoViewWin() {
    return g_noViewWin;
}
uint64_t    dbgNoViewLayer() {
    return g_noViewLayer;
}
uint64_t    dbgNoViewOther() {
    return g_noViewOther;
}
uint64_t    dbgNoEffect() {
    return g_noEffect;
}
uint64_t    dbgEmptyRegion() {
    return g_emptyRegion;
}

} // namespace GlassBlur
