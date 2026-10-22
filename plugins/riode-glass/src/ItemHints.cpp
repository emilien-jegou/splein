// See ItemHints.hpp.

#include "Format.hpp"
#include "ItemHints.hpp"
#include "item-helper/riode_item_helper_api.h"

#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/protocols/core/Compositor.hpp>

#include <dlfcn.h>
#include <fcntl.h>
#include <format>
#include <cstring>
#include <sys/mman.h>
#include <unistd.h>
#include <unordered_map>

namespace {

extern "C" {
extern const unsigned char riodeItemHelperBlobStart[];
extern const unsigned char riodeItemHelperBlobEnd[];
}

const struct riode_item_helper_v1_api* g_api = nullptr;

struct SSurfaceListeners {
    Hyprutils::Signal::CHyprSignalListener precommit;
    Hyprutils::Signal::CHyprSignalListener commit;
};

// One entry per surface with an active item, keyed by the raw wl_resource.
std::unordered_map<wl_resource*, SSurfaceListeners> g_surfaceListeners;

void onItemDestroyed(wl_resource* surfaceResource, void*) {
    g_surfaceListeners.erase(surfaceResource);
}

// An item object now exists for this surface: attach snapshot/apply
// BEFORE the client's first hint commit. Lazy attachment (the GlassBlur
// draw path) runs only after that commit has already gone through, and a
// static client never sends another one — its hints would sit unapplied
// in the helper's `pending` forever (default-blur, no optics).
void onItemCreated(wl_resource* surfaceResource, void*) {
    ItemHints::ensureTracked(surfaceResource);
}

static std::string g_initState = "pending";
static std::string g_loadDetail = "none";

bool loadHelper(HANDLE handle) {
    // Fresh-first: always map this build's own helper blob. A resident
    // older generation (RTLD_NODELETE — it can never be unloaded) must
    // not serve a newer plugin: even with matching ABIs its globals bind
    // against whatever interface descriptors were first in load order.
    // Adopt is a fallback only, for when the fresh load itself fails.
    bool freshOk = false;
    {
        const auto blobSize = static_cast<size_t>(riodeItemHelperBlobEnd - riodeItemHelperBlobStart);
        if (blobSize >= 1000) {
            // Unique memfd name AND a kept-open fd: glibc's dlopen
            // de-duplicates by PATH STRING, and a closed fd lets the
            // number be recycled — the next loader would then get the
            // PREVIOUS object back (stale helper, wrong ABI). Same
            // discipline as the shader glue loader.
            static unsigned g_helperSeq = 0;
            char blobName[64];
            std::snprintf(blobName, sizeof(blobName), "riode-item-helper-%d-%u", (int)getpid(), g_helperSeq++);
            int fd = memfd_create(blobName, MFD_CLOEXEC);
            if (fd >= 0) {
                auto* data = reinterpret_cast<const char*>(riodeItemHelperBlobStart);
                size_t written = 0;
                while (written < blobSize) {
                    ssize_t n = write(fd, data + written, blobSize - written);
                    if (n <= 0)
                        break;
                    written += static_cast<size_t>(n);
                }
                if (written == blobSize) {
                    const auto fdPath = rfmt::format("/proc/self/fd/{}", fd);
                    dlerror();
                    if (void* h = dlopen(fdPath.c_str(), RTLD_NOW | RTLD_GLOBAL | RTLD_NODELETE)) {
                        if (const auto* fresh = static_cast<const struct riode_item_helper_v1_api*>(
                                dlsym(h, "riode_item_helper_v1_api"))) {
                            if (fresh->abi_version == RIODE_ITEM_HELPER_V1_ABI_VERSION) {
                                g_api = fresh;
                                g_loadDetail = rfmt::format("fresh-blob api={} start={}", (const void*)g_api,
                                                           (void*)g_api->start);
                                freshOk = true;
                            } else {
                                g_loadDetail = rfmt::format("fresh-blob-abi-mismatch={} (want {})",
                                                           fresh->abi_version,
                                                           RIODE_ITEM_HELPER_V1_ABI_VERSION);
                            }
                        } else {
                            const char* err = dlerror();
                            g_loadDetail = rfmt::format("fresh-dlsym-failed:{}", err ? err : "?");
                        }
                    } else {
                        const char* err = dlerror();
                        g_loadDetail = rfmt::format("fresh-dlopen-failed:{}", err ? err : "?");
                    }
                } else {
                    g_loadDetail = rfmt::format("fresh-write-short={}/{}", written, blobSize);
                }
                // NOTE: the fd is intentionally NOT closed — the
                // /proc/self/fd path must stay unique for this object
                // (see the memfd name comment above).
            } else {
                g_loadDetail = "fresh-memfd-failed";
            }
        } else {
            g_loadDetail = rfmt::format("fresh-blob-too-small={}", blobSize);
        }
        if (freshOk) {
            g_initState = "fresh";
            return true;
        }
    }

    if (void* adopted = dlsym(RTLD_DEFAULT, "riode_item_helper_v1_api")) {
        g_api = static_cast<const struct riode_item_helper_v1_api*>(adopted);
        if (g_api && g_api->abi_version == RIODE_ITEM_HELPER_V1_ABI_VERSION) {
            g_initState = "adopted";
            g_loadDetail = rfmt::format("adopted-resident api={} start={} (fresh failed: {})", (void*)g_api,
                                       (void*)g_api->start, g_loadDetail);
            return true;
        }
        g_loadDetail += rfmt::format(" | adopt-rejected-abi-{}", g_api ? g_api->abi_version : 9999);
        g_api = nullptr;
    } else {
        g_loadDetail += " | adopt-no-resident";
    }

    // Both fresh-first and adopt failed (or adopt found only an
    // incompatible generation): no usable helper in this process.
    g_initState = "no-helper";
    HyprlandAPI::addNotification(handle, "[riode-glass] item helper unavailable — per-item hints disabled",
                                 CHyprColor{1.0, 0.8, 0.2, 1.0}, 5000);
    return false;
}

std::string formatShape(const riode_item_state& s) {
    if (s.shape_mode != RIODE_ITEM_SHAPE_EXPLICIT)
        return "none";
    return rfmt::format("{:.0f},{:.0f},{:.0f}x{:.0f} {:.0f},{:.0f},{:.0f},{:.0f}", s.x, s.y, s.width, s.height, s.radii[0],
                       s.radii[1], s.radii[2], s.radii[3]);
}

// The P1 field copy retired with the v1 wire: packed frames ride
// through untouched (the optics shader consumes them directly in 8.4).
static_assert(RIODE_ITEM_HELPER_GLASS_PACKED_FLOATS == 792,
              "pack size must match K_GLASS_UNIFORM_FLOATS");

} // namespace

namespace ItemHints {

bool init(HANDLE handle) {
    if (!loadHelper(handle)) {
        HyprlandAPI::addNotification(handle, "[riode-glass] item helper unavailable — per-item hints disabled",
                                     CHyprColor{1.0, 0.8, 0.2, 1.0}, 5000);
        return false;
    }
    g_api->set_callbacks(nullptr, onItemCreated, onItemDestroyed);
    if (g_api->start(g_pCompositor->m_wlDisplay) != 0) {
        // Capture the failing instance's own introspection BEFORE
        // dropping it: manager/item counts reveal whether this is a
        // stale adopted generation rather than the fresh blob.
        g_loadDetail += std::string(" start-failed-self=[") + (g_api->debug_state ? g_api->debug_state() : "?") + "]";
                        + " display=" + std::to_string((unsigned long long)g_pCompositor->m_wlDisplay);
        g_api = nullptr;
        g_initState = "start";
        HyprlandAPI::addNotification(handle, "[riode-glass] item helper start() failed — no global",
                                     CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        HyprlandAPI::addNotification(handle, "[riode-glass] item helper start() failed — no riode_item_manager global",
                                     CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        return false;
    }
    HyprlandAPI::addNotification(handle,
                                 rfmt::format("[riode-glass] item helper ABI {} live", RIODE_ITEM_HELPER_V1_ABI_VERSION),
                                 CHyprColor{0.2, 0.9, 0.4, 1.0}, 3000);
    // Recover listeners for items left behind by a load that never ran its
    // exit path: re-attach snapshot/apply so their state keeps latching.
    g_api->for_each_active_item(
        [](wl_resource* surface, void*) {
            ensureTracked(surface);
        },
        nullptr);
    return true;
}

void shutdown(HANDLE) {
    if (!g_api)
        return;
    g_api->set_callbacks(nullptr, nullptr, nullptr);
    g_api->stop();
    g_api = nullptr;
    g_surfaceListeners.clear();
}

bool live() {
    return g_api != nullptr;
}

void counts(int& managers, int& total, int& active) {
    managers = total = active = 0;
    if (!g_api)
        return;
    managers = g_api->manager_count();
    g_api->item_counts(&total, &active);
}

std::string debugState() {
    if (!g_api)
        return "no-helper";
    return g_api->debug_state();
}

std::string initState() {
    return g_initState;
}

std::string loadDetail() {
    return g_loadDetail;
}

void ensureTracked(wl_resource* surface) {
    if (!g_api || !surface || g_surfaceListeners.contains(surface))
        return;
    auto& listeners = g_surfaceListeners[surface];
    // Snapshot/apply need the CWLSurfaceResource signals; the wire resource
    // maps back via fromResource.
    if (auto hlSurface = CWLSurfaceResource::fromResource(surface)) {
        listeners.precommit = hlSurface->m_events.precommit.listen([surface] {
            if (g_api)
                g_api->snapshot(surface);
        });
        listeners.commit = hlSurface->m_events.commit.listen([surface] {
            if (g_api)
                g_api->apply(surface);
        });
        // Late attachment: the client's (possibly only) hint commit may
        // have happened before these listeners existed. The helper
        // retains `pending`, so apply it now — then per-commit latching
        // takes over. Both calls run on the compositor thread, outside
        // request dispatch (draw path), or at item creation (before any
        // set_glass can exist) — never mid batch.
        if (g_api) {
            g_api->snapshot(surface);
            g_api->apply(surface);
        }
    } else {
        // No Hyprland-side surface (yet): drop the entry; the next frame's
        // ensureTracked retries until the resource resolves.
        g_surfaceListeners.erase(surface);
    }
}

bool get(wl_resource* surface, ItemInfo& infoOut) {
    if (!g_api || !surface)
        return false;
    struct riode_item_state state = {};
    if (!g_api->get(surface, &state))
        return false;
    infoOut = ItemInfo{};
    if (state.glass_mode == RIODE_ITEM_GLASS_MATERIAL) {
        // v5 packed frame: copy through untouched to the optics pass.
        infoOut.has_packed = true;
        infoOut.pack_sigma_x = state.glass_sigma_x;
        infoOut.pack_sigma_y = state.glass_sigma_y;
        infoOut.pack_box_x = state.glass_box_x;
        infoOut.pack_box_y = state.glass_box_y;
        infoOut.pack_box_w = state.glass_box_w;
        infoOut.pack_box_h = state.glass_box_h;
        static_assert(sizeof(infoOut.packed) == sizeof(state.glass_packed),
                      "packed frame size must match the helper state");
        memcpy(infoOut.packed, state.glass_packed, sizeof(infoOut.packed));
    }
    infoOut.shape = formatShape(state);
    // v3 effect stages: bounded POD slots -> owned copies.
    infoOut.glass_stack_index = state.glass_stack_index;
    infoOut.effects.clear();
    infoOut.effects.reserve(state.effect_count);
    for (uint32_t i = 0; i < state.effect_count && i < RIODE_ITEM_HELPER_MAX_EFFECTS; ++i) {
        const auto& s = state.effects[i];
        if (s.wgsl_len == 0 || s.wgsl_len >= RIODE_ITEM_HELPER_EFFECT_WGSL_MAX)
            continue;
        ItemInfo::EffectStage e;
        e.stack_index    = s.stack_index;
        e.backdrop_need  = s.backdrop_need;
        e.backdrop_sigma = s.backdrop_sigma;
        e.wgsl.assign(s.wgsl, s.wgsl_len);
        e.uniforms.assign(s.uniforms, s.uniforms + s.uniform_count);
        infoOut.effects.push_back(std::move(e));
    }
    return true;
}

namespace {
void collectOne(wl_resource* surface, void* userdata) {
    auto*    out = static_cast<std::vector<ItemInfo>*>(userdata);
    ItemInfo info;
    if (ItemHints::get(surface, info))
        out->push_back(info);
}
} // namespace

std::vector<ItemInfo> activeItems() {
    std::vector<ItemInfo> out;
    if (g_api)
        g_api->for_each_active_item(collectOne, &out);
    return out;
}

} // namespace ItemHints
