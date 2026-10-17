#pragma once

// riode_item_v1 transport on the plugin side: loads the persistent helper,
// advances item state on surface precommit/commit, and exposes latched
// state for rendering and `riode status`.
//
// The helper owns all protocol objects; this module only shuttles surface
// lifetimes (listeners) and reads snapshots.

#include <hyprland/src/plugins/PluginAPI.hpp>

#include "item-helper/riode_item_helper_api.h"

#include "GlassOptics.hpp"

#include <string>
#include <vector>

namespace ItemHints {

struct ItemInfo {
    // v5 packed frame: present when the helper latched a packed upload
    // (plain-blur fallback without it).
    bool  has_packed = false;
    float pack_sigma_x = 0.0f;
    float pack_sigma_y = 0.0f;
    float pack_box_x = 0.0f;
    float pack_box_y = 0.0f;
    float pack_box_w = 0.0f;
    float pack_box_h = 0.0f;
    float packed[RIODE_ITEM_HELPER_GLASS_PACKED_FLOATS] = {};
    // "none" or "x,y,wxh r1,r2,r3,r4"
    std::string shape;
};

/// Loads (or adopts) the helper and starts its global. False = protocol
/// unavailable; blur keeps working, hints stay empty.
bool init(HANDLE handle);
/// Detaches callbacks and stops the helper's global. Safe when not inited.
void shutdown(HANDLE handle);
/// True while the helper API is loaded (regardless of items).
bool live();
/// Machine-readable init failure stage: "ok", "adopted", "memfd", "write",
/// "dlopen:<err>", "symbol", "abi", "start".
std::string initState();
/// Debug introspection: bound managers, total items, active items.
void counts(int& managers, int& total, int& active);
/// Debug snapshot of the helper's tables (global/token/managers/items).
std::string debugState();
/// Machine-readable init path (ok/adopted/fresh vs memfd/write/dlopen/symbol/start).
std::string initState();
/// Which helper instance was picked up (adopted-resident vs fresh-blob,
/// plus rejected ABI versions seen along the way).
std::string loadDetail();
/// Attaches snapshot/apply listeners to a surface resource once.
void ensureTracked(struct wl_resource* surface);
/// Latched state for one surface; false when it has no active item.
bool get(struct wl_resource* surface, ItemInfo& infoOut);
/// All currently active items (for status).
std::vector<ItemInfo> activeItems();

} // namespace ItemHints
