// Persistent riode_item_v1 protocol helper.
//
// Built as its own shared library and never dlclose'd (see ItemHelperBlob.S
// and the plugin's loader): every wl_global/wl_resource/wl_interface this
// file owns stays valid memory across a riode-glass.so unload+reload. That
// is what lets an object become permanently inert instead of being destroyed
// out from under a client that is still mid-request.
//
// Depends only on libwayland-server: no Hyprland headers, no C++.
// Adapted from the hyprglass item helper (set_inherit_shape dropped:
// riode_item_v1 has no such request).

#include "riode-item-v1-server-protocol.h"
#include "riode_item_helper_api.h"

#include <wayland-server.h>
#include <wayland-server-protocol.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define RIODE_HELPER_LOG(fmt, ...) fprintf(stderr, "[riode-item-helper] " fmt "\n", ##__VA_ARGS__)

struct manager_object {
    struct wl_resource* resource;
    int                 inert;
    struct wl_list      link; // in g_managers
};

struct item_object {
    struct wl_resource* resource;
    struct wl_resource* surface; // borrowed; NULL once the surface has died
    struct wl_listener  surface_destroy_listener;
    int                 listening;
    int                 inert;

    struct riode_item_state pending;
    struct riode_item_state cached;
    struct riode_item_state current;

    struct wl_list link; // in g_items
};

struct retired_global {
    struct wl_global* global;
    struct wl_list    link; // in g_retired
};

static struct wl_list g_managers;
static struct wl_list g_items;
static struct wl_list g_retired;
static int            g_lists_initialized;

static struct wl_global* g_active_global;

static void*                     g_callback_userdata;
static riode_item_created_fn     g_on_created;
static riode_item_destroyed_fn   g_on_destroyed;

// ── wl_interface descriptors (what wayland-scanner's private-code output
// would generate from protocols/riode-item-v1.xml) ─────────────────────────

// Forward declarations: the type tables reference the descriptors defined
// below. Static throughout (see below) so resident older generations can
// never interpose them.
static const struct wl_interface riode_item_manager_v1_interface;
static const struct wl_interface riode_item_v1_interface;

static const struct wl_interface* riode_item_manager_v1_types[] = {
    &riode_item_v1_interface,
    &wl_surface_interface,
};

static const struct wl_message riode_item_manager_v1_requests[] = {
    {"destroy", "", riode_item_manager_v1_types + 2},
    {"get_item", "no", riode_item_manager_v1_types + 0},
};

static const struct wl_message riode_item_manager_v1_events[] = {
    {"finished", "", riode_item_manager_v1_types + 2},
};

// Static (internal linkage): a previous helper generation stays resident
// forever (RTLD_NODELETE) and exports the same symbol names. A global
// definition here would be interposed by the older resident copy at
// load-order priority, silently downgrading this instance's interface
// version (v2 request against a v1 descriptor → wl_global_create
// refuses). Static keeps each generation on its own descriptors.
static const struct wl_interface riode_item_manager_v1_interface = {"riode_item_manager_v1", 2, 2, riode_item_manager_v1_requests, 1,
                                                             riode_item_manager_v1_events};

static const struct wl_message riode_item_v1_requests[] = {
    {"destroy", "", NULL},
    {"set_glass", "a", NULL},
    {"unset_glass", "", NULL},
    {"set_shape", "ffffffff", NULL},
    {"unset_shape", "", NULL},
};

static const struct wl_interface riode_item_v1_interface = {"riode_item_v1", 2, 5, riode_item_v1_requests, 0, NULL};

// ── internals ───────────────────────────────────────────────────────────────

static void ensure_lists_initialized(void) {
    if (g_lists_initialized)
        return;
    g_lists_initialized = 1;
    wl_list_init(&g_managers);
    wl_list_init(&g_items);
    wl_list_init(&g_retired);
}

static struct item_object* find_active_item_for_surface(struct wl_resource* surface) {
    struct item_object* item;
    wl_list_for_each(item, &g_items, link) {
        if (!item->inert && item->surface == surface)
            return item;
    }
    return NULL;
}

// ── riode_item_v1 (item object) request handlers ────────────────────────────

static void item_handle_destroy(struct wl_client* client, struct wl_resource* resource) {
    (void)client;
    wl_resource_destroy(resource);
}

static uint32_t read_le_u32(const unsigned char* p) {
    return ((uint32_t)p[0]) | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static void clear_pending_glass(struct item_object* item) {
    item->pending.glass_mode = RIODE_ITEM_GLASS_NONE;
    item->pending.glass_version = 0;
    item->pending.glass_sigma_x = 0;
    item->pending.glass_sigma_y = 0;
    item->pending.glass_box_x = 0;
    item->pending.glass_box_y = 0;
    item->pending.glass_box_w = 0;
    item->pending.glass_box_h = 0;
    memset(item->pending.glass_packed, 0, sizeof(item->pending.glass_packed));
}

// Decodes the v5 packed glass frame: version tag, frame blur σx/σy,
// integer sample box (x0, y0, w, h, device px), then the full uniform
// stream (prefix + glow tail) the optics shader consumes directly
// (box-relative). Exact-size match only — the frame is
// fixed-size, so anything else (v1 P1 material included) is a corrupt
// or foreign payload and clears the hint. Hints are sanitised, never
// protocol errors.
static void item_handle_set_glass(struct wl_client* client, struct wl_resource* resource, struct wl_array* material) {
    (void)client;
    struct item_object* item = wl_resource_get_user_data(resource);
    if (item->inert)
        return;

    clear_pending_glass(item);
    if (!material || !material->data || (material->size % 4) != 0)
        return;
    size_t words = material->size / 4;
    if (words != RIODE_ITEM_HELPER_GLASS_PACKED_WORDS)
        return;
    const unsigned char* bytes = material->data;
    if (read_le_u32(bytes) != 5u) // engine pack version
        return;

    item->pending.glass_mode = RIODE_ITEM_GLASS_MATERIAL;
    item->pending.glass_version = 5u;
    uint32_t bits;
    float value;
#define READ_FLOAT_AT(word, out)                       \
    do {                                               \
        bits = read_le_u32(bytes + (word) * 4);        \
        memcpy(&value, &bits, sizeof(value));          \
        (out) = value;                                 \
    } while (0)
    READ_FLOAT_AT(1, item->pending.glass_sigma_x);
    READ_FLOAT_AT(2, item->pending.glass_sigma_y);
    READ_FLOAT_AT(3, item->pending.glass_box_x);
    READ_FLOAT_AT(4, item->pending.glass_box_y);
    READ_FLOAT_AT(5, item->pending.glass_box_w);
    READ_FLOAT_AT(6, item->pending.glass_box_h);
    for (size_t i = 0; i < RIODE_ITEM_HELPER_GLASS_PACKED_FLOATS; ++i) {
        bits = read_le_u32(bytes + (7 + i) * 4);
        memcpy(&value, &bits, sizeof(value));
        item->pending.glass_packed[i] = value;
    }
#undef READ_FLOAT_AT
}

static void item_handle_unset_glass(struct wl_client* client, struct wl_resource* resource) {
    (void)client;
    struct item_object* item = wl_resource_get_user_data(resource);
    if (item->inert)
        return;
    clear_pending_glass(item);
}

static void clear_pending_shape(struct item_object* item) {
    item->pending.shape_mode = RIODE_ITEM_SHAPE_NONE;
    item->pending.x = item->pending.y = item->pending.width = item->pending.height = 0.0;
    item->pending.radii[0] = item->pending.radii[1] = item->pending.radii[2] = item->pending.radii[3] = 0.0;
}

static void item_handle_set_shape(struct wl_client* client, struct wl_resource* resource, wl_fixed_t x, wl_fixed_t y, wl_fixed_t width,
                                   wl_fixed_t height, wl_fixed_t radius_top_left, wl_fixed_t radius_top_right,
                                   wl_fixed_t radius_bottom_right, wl_fixed_t radius_bottom_left) {
    (void)client;
    struct item_object* item = wl_resource_get_user_data(resource);
    if (item->inert)
        return;

    double w = wl_fixed_to_double(width);
    double h = wl_fixed_to_double(height);
    if (w <= 0.0 || h <= 0.0) {
        clear_pending_shape(item);
        return;
    }

    item->pending.shape_mode = RIODE_ITEM_SHAPE_EXPLICIT;
    item->pending.x          = wl_fixed_to_double(x);
    item->pending.y          = wl_fixed_to_double(y);
    item->pending.width      = w;
    item->pending.height     = h;

    double radii[4] = {wl_fixed_to_double(radius_top_left), wl_fixed_to_double(radius_top_right), wl_fixed_to_double(radius_bottom_right),
                       wl_fixed_to_double(radius_bottom_left)};
    for (int i = 0; i < 4; ++i)
        item->pending.radii[i] = radii[i] < 0.0 ? 0.0 : radii[i];
}

static void item_handle_unset_shape(struct wl_client* client, struct wl_resource* resource) {
    (void)client;
    struct item_object* item = wl_resource_get_user_data(resource);
    if (item->inert)
        return;
    clear_pending_shape(item);
}

static const struct riode_item_v1_interface item_impl = {
    .destroy      = item_handle_destroy,
    .set_glass    = item_handle_set_glass,
    .unset_glass  = item_handle_unset_glass,
    .set_shape    = item_handle_set_shape,
    .unset_shape  = item_handle_unset_shape,
};

static void handle_surface_destroy(struct wl_listener* listener, void* data) {
    (void)data;
    struct item_object* item;
    item = wl_container_of(listener, item, surface_destroy_listener);

    struct wl_resource* surface = item->surface;
    item->listening             = 0;
    item->inert                 = 1;
    item->surface               = NULL;

    if (g_on_destroyed)
        g_on_destroyed(surface, g_callback_userdata);
}

static void item_resource_destroy(struct wl_resource* resource) {
    struct item_object* item = wl_resource_get_user_data(resource);

    if (item->listening) {
        wl_list_remove(&item->surface_destroy_listener.link);
        item->listening = 0;
    }

    if (!item->inert && item->surface && g_on_destroyed)
        g_on_destroyed(item->surface, g_callback_userdata);

    wl_list_remove(&item->link);
    free(item);
}

// ── riode_item_manager_v1 (manager object) request handlers ─────────────────

static void manager_handle_destroy(struct wl_client* client, struct wl_resource* resource) {
    (void)client;
    wl_resource_destroy(resource);
}

static void manager_handle_get_item(struct wl_client* client, struct wl_resource* manager_resource, uint32_t id,
                                    struct wl_resource* surface_resource) {
    struct manager_object* manager = wl_resource_get_user_data(manager_resource);

    struct wl_resource* item_resource = wl_resource_create(client, &riode_item_v1_interface, wl_resource_get_version(manager_resource), id);
    if (!item_resource) {
        wl_client_post_no_memory(client);
        return;
    }

    if (!manager->inert) {
        struct item_object* existing = find_active_item_for_surface(surface_resource);
        if (existing) {
            wl_resource_post_error(manager_resource, RIODE_ITEM_MANAGER_V1_ERROR_ITEM_EXISTS, "surface already has an active riode_item_v1 object");
            wl_resource_destroy(item_resource);
            return;
        }
    }

    struct item_object* item = calloc(1, sizeof(*item));
    if (!item) {
        wl_resource_destroy(item_resource);
        wl_client_post_no_memory(client);
        return;
    }

    item->resource = item_resource;
    item->surface  = surface_resource;
    item->inert    = manager->inert;
    RIODE_HELPER_LOG("get_item inert=%d", item->inert);

    wl_list_insert(&g_items, &item->link);
    wl_resource_set_implementation(item_resource, &item_impl, item, item_resource_destroy);

    if (!item->inert) {
        item->surface_destroy_listener.notify = handle_surface_destroy;
        wl_resource_add_destroy_listener(surface_resource, &item->surface_destroy_listener);
        item->listening = 1;

        if (g_on_created)
            g_on_created(surface_resource, g_callback_userdata);
    }
}

static const struct riode_item_manager_v1_interface manager_impl = {
    .destroy  = manager_handle_destroy,
    .get_item = manager_handle_get_item,
};

static void manager_resource_destroy(struct wl_resource* resource) {
    struct manager_object* manager = wl_resource_get_user_data(resource);
    wl_list_remove(&manager->link);
    free(manager);
}

static void bind_manager(struct wl_client* client, void* data, uint32_t version, uint32_t id) {
    // A bind that reaches this handler is for a live global by
    // construction: binds for removed globals fail lookup before
    // dispatch. So no inert-at-bind check — every manager starts active.
    // (A previous revision compared bind-time `data` against a generation
    // token, but the comparison marked EVERYTHING inert; the token never
    // matches what the bind callback actually receives.)
    (void)data;
    struct wl_resource*      resource      = wl_resource_create(client, &riode_item_manager_v1_interface, (int)version, id);
    if (!resource) {
        wl_client_post_no_memory(client);
        return;
    }

    struct manager_object* manager = calloc(1, sizeof(*manager));
    if (!manager) {
        wl_resource_destroy(resource);
        wl_client_post_no_memory(client);
        return;
    }

    manager->resource = resource;
    manager->inert    = 0;
    RIODE_HELPER_LOG("bind_manager version=%d inert=%d", version, manager->inert);

    wl_list_insert(&g_managers, &manager->link);
    wl_resource_set_implementation(resource, &manager_impl, manager, manager_resource_destroy);
}

// ── Public API ──────────────────────────────────────────────────────────────

static int api_start(struct wl_display* display) {
    ensure_lists_initialized();

    if (g_active_global)
        return 0;

    struct retired_global *retired, *retired_tmp;
    wl_list_for_each_safe(retired, retired_tmp, &g_retired, link) {
        wl_global_destroy(retired->global);
        wl_list_remove(&retired->link);
        free(retired);
    }

    struct wl_global* global = wl_global_create(display, &riode_item_manager_v1_interface, 2, NULL, bind_manager);
    if (!global)
        return -1;

    wl_global_set_user_data(global, global);
    g_active_global = global;
    return 0;
}

static void api_stop(void) {
    ensure_lists_initialized();

    if (!g_active_global)
        return;

    struct manager_object* manager;
    wl_list_for_each(manager, &g_managers, link) {
        if (manager->inert)
            continue;
        manager->inert = 1;
        riode_item_manager_v1_send_finished(manager->resource);
    }

    struct item_object* item;
    wl_list_for_each(item, &g_items, link) {
        if (item->listening) {
            wl_list_remove(&item->surface_destroy_listener.link);
            item->listening = 0;
        }
        item->inert   = 1;
        item->surface = NULL;
    }

    wl_global_remove(g_active_global);

    struct retired_global* retired = calloc(1, sizeof(*retired));
    if (!retired) {
        wl_global_destroy(g_active_global);
    } else {
        retired->global = g_active_global;
        wl_list_insert(&g_retired, &retired->link);
    }

    g_active_global = NULL;
}

static void api_set_callbacks(void* userdata, riode_item_created_fn on_created, riode_item_destroyed_fn on_destroyed) {
    g_callback_userdata = userdata;
    g_on_created        = on_created;
    g_on_destroyed      = on_destroyed;
}

static void api_snapshot(struct wl_resource* surface) {
    ensure_lists_initialized();
    struct item_object* item = find_active_item_for_surface(surface);
    if (item)
        item->cached = item->pending;
}

static void api_apply(struct wl_resource* surface) {
    ensure_lists_initialized();
    struct item_object* item = find_active_item_for_surface(surface);
    if (item)
        item->current = item->cached;
}

static int api_get(struct wl_resource* surface, struct riode_item_state* out_state) {
    ensure_lists_initialized();
    struct item_object* item = find_active_item_for_surface(surface);
    if (!item)
        return 0;
    *out_state = item->current;
    return 1;
}

static void api_for_each_active_item(riode_item_foreach_fn callback, void* userdata) {
    ensure_lists_initialized();
    struct item_object* item;
    wl_list_for_each(item, &g_items, link) {
        if (!item->inert && item->surface)
            callback(item->surface, userdata);
    }
}

static int api_manager_count(void) {
    ensure_lists_initialized();
    int n = 0;
    struct manager_object* manager;
    wl_list_for_each(manager, &g_managers, link) {
        (void)manager;
        n++;
    }
    return n;
}

static const char* api_debug_state(void) {
    static char buf[2048];
    int         n    = 0;
    int         left = (int)sizeof(buf);
    n += snprintf(buf + n, (size_t)left > 0 ? (size_t)left : 0, "active_global=%p ", (void*)g_active_global);
    left = (int)sizeof(buf) - n;
    struct manager_object* manager;
    wl_list_for_each(manager, &g_managers, link) {
        if (left <= 0)
            break;
        n += snprintf(buf + n, (size_t)left, "mgr[%p inert=%d] ", (void*)manager->resource, manager->inert);
        left = (int)sizeof(buf) - n;
    }
    struct item_object* item;
    wl_list_for_each(item, &g_items, link) {
        if (left <= 0)
            break;
        n += snprintf(buf + n, (size_t)left, "item[%p->%p inert=%d] ", (void*)item->resource, (void*)item->surface, item->inert);
        left = (int)sizeof(buf) - n;
    }
    return buf;
}

static int api_item_counts(int* total_out, int* active_out) {
    ensure_lists_initialized();
    int total = 0, active = 0;
    struct item_object* item;
    wl_list_for_each(item, &g_items, link) {
        total++;
        if (!item->inert && item->surface)
            active++;
    }
    if (total_out)
        *total_out = total;
    if (active_out)
        *active_out = active;
    return total;
}

const struct riode_item_helper_v1_api riode_item_helper_v1_api = {
    .abi_version           = RIODE_ITEM_HELPER_V1_ABI_VERSION,
    .start                 = api_start,
    .stop                  = api_stop,
    .set_callbacks         = api_set_callbacks,
    .snapshot              = api_snapshot,
    .apply                 = api_apply,
    .get                   = api_get,
    .for_each_active_item  = api_for_each_active_item,
    .manager_count         = api_manager_count,
    .item_counts           = api_item_counts,
    .debug_state           = api_debug_state,
};
