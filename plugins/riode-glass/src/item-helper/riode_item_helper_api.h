// Stable ABI between the persistent riode_item_v1 protocol helper (built
// as its own shared library, embedded in and loaded by riode-glass.so, but
// never unmapped across a plugin reload) and the reloadable plugin itself.
// Plain C so both a plain-C translation unit (the helper) and a C++ one
// (the plugin) can share it.

#ifndef RIODE_ITEM_HELPER_API_H
#define RIODE_ITEM_HELPER_API_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

struct wl_display;
struct wl_resource;

// Bumped to 7 (wire v5: the packed frame carries the full uniform
// stream — the 736-float prefix plus the 56-float glow tail the optics
// shader's glow/blend stages consume; header layout unchanged).
#define RIODE_ITEM_HELPER_V1_ABI_VERSION 7u

// v5 packed frame: version tag + frame blur σx/σy + integer sample box
// (x0, y0, w, h, device px) + the full uniform stream (prefix + glow
// tail) the optics shader consumes directly (box-relative). The engine
// packs (`pack_glass_wire`); the plugin uploads blindly — no C++ pack to
// drift from the Rust one.
#define RIODE_ITEM_HELPER_GLASS_PACKED_WORDS 799
#define RIODE_ITEM_HELPER_GLASS_PACKED_FLOATS 792

// Largest accepted set_glass payload in u32 words. Exact match only:
// the packed frame is fixed-size, so anything else is a corrupt or
// foreign payload (v1 P1 material included) and clears the hint.
#define RIODE_ITEM_HELPER_GLASS_MAX_WORDS RIODE_ITEM_HELPER_GLASS_PACKED_WORDS

enum riode_item_glass_mode {
    RIODE_ITEM_GLASS_NONE     = 0, // plain-blur fallback
    RIODE_ITEM_GLASS_MATERIAL = 1, // decoded engine material below
};

enum riode_item_shape_mode {
    RIODE_ITEM_SHAPE_NONE     = 0, // region extents, square corners
    RIODE_ITEM_SHAPE_EXPLICIT = 1, // x/y/width/height + radii below
};

// Plain C mirror of one surface's latched (post-commit) item state.
struct riode_item_state {
    enum riode_item_glass_mode glass_mode;
    uint32_t                   glass_version; // engine pack version (5)
    float glass_sigma_x; // frame blur σx, device px
    float glass_sigma_y; // frame blur σy, device px
    float glass_box_x;   // integer sample-box origin x0, device px
    float glass_box_y;   // integer sample-box origin y0, device px
    float glass_box_w;   // sample-box width, device px
    float glass_box_h;   // sample-box height, device px
    float glass_packed[RIODE_ITEM_HELPER_GLASS_PACKED_FLOATS];
    enum riode_item_shape_mode shape_mode;
    double                     x, y, width, height; // surface-local logical px, EXPLICIT only
    double                     radii[4];           // TL, TR, BR, BL, EXPLICIT only
};

typedef void (*riode_item_created_fn)(struct wl_resource* surface, void* userdata);
typedef void (*riode_item_destroyed_fn)(struct wl_resource* surface, void* userdata);
typedef void (*riode_item_foreach_fn)(struct wl_resource* surface, void* userdata);

struct riode_item_helper_v1_api {
    uint32_t abi_version;
    int (*start)(struct wl_display* display);
    void (*stop)(void);
    void (*set_callbacks)(void* userdata, riode_item_created_fn on_created, riode_item_destroyed_fn on_destroyed);
    void (*snapshot)(struct wl_resource* surface);
    void (*apply)(struct wl_resource* surface);
    int (*get)(struct wl_resource* surface, struct riode_item_state* out_state);
    void (*for_each_active_item)(riode_item_foreach_fn callback, void* userdata);

    // Introspection (debug): bound managers (active + inert) and items
    // (total ever created in this instance, and currently active).
    int (*manager_count)(void);
    int (*item_counts)(int* total_out, int* active_out);
    // Debug snapshot: active global/token and every manager+item with
    // inert flags. Returned pointer valid until the next call.
    const char* (*debug_state)(void);
};

extern const struct riode_item_helper_v1_api riode_item_helper_v1_api;

#ifdef __cplusplus
}
#endif

#endif
