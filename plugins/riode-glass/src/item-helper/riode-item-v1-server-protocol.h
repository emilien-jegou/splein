// Hand-written equivalent of wayland-scanner server-header output for
// protocols/riode-item-v1.xml (no wayland-scanner binary in the closure,
// so codegen runs nowhere). Message order, signatures, opcodes and error
// codes mirror the XML 1:1; verify against the XML if either changes.

#ifndef RIODE_ITEM_V1_SERVER_PROTOCOL_H
#define RIODE_ITEM_V1_SERVER_PROTOCOL_H

#include <stdint.h>
#include <stddef.h>
#include "wayland-server.h"

#ifdef __cplusplus
extern "C" {
#endif

struct wl_client;
struct wl_resource;

struct riode_item_manager_v1;
struct riode_item_v1;

#ifndef RIODE_ITEM_MANAGER_V1_ERROR_ENUM
#define RIODE_ITEM_MANAGER_V1_ERROR_ENUM
enum riode_item_manager_v1_error {
    RIODE_ITEM_MANAGER_V1_ERROR_ITEM_EXISTS = 0,
};
#endif /* RIODE_ITEM_MANAGER_V1_ERROR_ENUM */

struct riode_item_manager_v1_interface {
    void (*destroy)(struct wl_client* client, struct wl_resource* resource);
    void (*get_item)(struct wl_client* client, struct wl_resource* resource, uint32_t id, struct wl_resource* surface);
};

#define RIODE_ITEM_MANAGER_V1_DESTROY            0
#define RIODE_ITEM_MANAGER_V1_GET_ITEM           1
#define RIODE_ITEM_MANAGER_V1_FINISHED           0
#define RIODE_ITEM_MANAGER_V1_DESTROY_SINCE_VERSION     1
#define RIODE_ITEM_MANAGER_V1_GET_ITEM_SINCE_VERSION    1
#define RIODE_ITEM_MANAGER_V1_FINISHED_SINCE_VERSION   1

static inline void
riode_item_manager_v1_send_finished(struct wl_resource* resource_) {
    wl_resource_post_event(resource_, RIODE_ITEM_MANAGER_V1_FINISHED);
}

#define RIODE_ITEM_MANAGER_V1_ERROR_ITEM_EXISTS  0

struct riode_item_v1_interface {
    void (*destroy)(struct wl_client* client, struct wl_resource* resource);
    void (*set_glass)(struct wl_client* client, struct wl_resource* resource, struct wl_array* material);
    void (*unset_glass)(struct wl_client* client, struct wl_resource* resource);
    void (*set_shape)(struct wl_client* client, struct wl_resource* resource, wl_fixed_t x, wl_fixed_t y, wl_fixed_t width,
                      wl_fixed_t height, wl_fixed_t radius_top_left, wl_fixed_t radius_top_right, wl_fixed_t radius_bottom_right,
                      wl_fixed_t radius_bottom_left);
    void (*unset_shape)(struct wl_client* client, struct wl_resource* resource);
    void (*set_effect)(struct wl_client* client, struct wl_resource* resource, int32_t stack_index, const char* wgsl,
                       struct wl_array* uniforms, uint32_t backdrop_need, wl_fixed_t backdrop_sigma);
    void (*clear_effects)(struct wl_client* client, struct wl_resource* resource);
    void (*set_glass_stage)(struct wl_client* client, struct wl_resource* resource, int32_t stack_index);
};

#define RIODE_ITEM_V1_DESTROY               0
#define RIODE_ITEM_V1_SET_GLASS             1
#define RIODE_ITEM_V1_UNSET_GLASS           2
#define RIODE_ITEM_V1_SET_SHAPE             3
#define RIODE_ITEM_V1_UNSET_SHAPE           4
#define RIODE_ITEM_V1_SET_EFFECT            5
#define RIODE_ITEM_V1_CLEAR_EFFECTS         6
#define RIODE_ITEM_V1_SET_GLASS_STAGE       7
#define RIODE_ITEM_V1_DESTROY_SINCE_VERSION      1
#define RIODE_ITEM_V1_SET_GLASS_SINCE_VERSION    2
#define RIODE_ITEM_V1_UNSET_GLASS_SINCE_VERSION  2
#define RIODE_ITEM_V1_SET_SHAPE_SINCE_VERSION    1
#define RIODE_ITEM_V1_UNSET_SHAPE_SINCE_VERSION  1
#define RIODE_ITEM_V1_SET_EFFECT_SINCE_VERSION   3
#define RIODE_ITEM_V1_CLEAR_EFFECTS_SINCE_VERSION 3
#define RIODE_ITEM_V1_SET_GLASS_STAGE_SINCE_VERSION 3

// No extern declarations for the interface descriptors on purpose: the
// helper defines them static (internal linkage) so a previous resident
// helper generation can never interpose them (see helper.c).

#ifdef __cplusplus
}
#endif

#endif
