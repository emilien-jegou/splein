// riode-glass — desktop-backdrop glass for riode apps (research-hyprland-plugin.md §5).
//
// IPC + rendering: version/hash guard (pause, never crash), one
// `hyprctl riode status` command, clean unload; the item-helper (embedded,
// ABI 7) latches v5 packed frames and GlassBlur/GlassOptics render the
// per-island backdrop blur, lens/frost optics, glow and blend modes —
// line-for-line with the wgpu twin in
// `crates/core/riode/src/foundation/effect/shaders/glass/`.
//
// Built against Hyprland 0.56.2 headers (see hyprpm.toml commit_pins).

#include "Format.hpp"
#include <hyprland/src/helpers/Color.hpp>
#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/plugins/PluginSystem.hpp>

#include "GlassBlur.hpp"
#include "GlassOptics.hpp"
#include "ItemHints.hpp"
#include "ShaderGlue.hpp"

#include <hyprland/src/Compositor.hpp>

#include <cstdlib>
#include <dlfcn.h>
#include <format>
#include <wayland-server.h>
#include <string>
#include <string_view>

namespace {

constexpr const char* PLUGIN_NAME = "riode-glass";
constexpr const char* PLUGIN_AUTHOR = "splein";
constexpr const char* PLUGIN_VERSION = "0.3.0";

// Built-against Hyprland pin. Keep in sync with hyprpm.toml commit_pins.
constexpr const char* BUILT_FOR_TAG = "v0.56.2";
constexpr const char* BUILT_FOR_HASH = "efb50993780079460b0cbed1363e2166a2de1d9f";

inline HANDLE PHANDLE = nullptr;
inline bool s_instanceActive = false;
inline SP<SHyprCtlCommand> s_command;

// Read by a later copy's duplicate check through dlsym.
APICALL EXPORT bool riode_glass_instance_active() {
    return s_instanceActive;
}

bool skipRequested() {
    const char* skip = std::getenv("RIODE_GLASS_SKIP_VERSION_CHECK");
    return skip && *skip && std::string_view{skip} != "0";
}

namespace {
// Probe: can this process create a v2 global at all? Isolates display/
// libwayland issues from helper-instance issues. Created and destroyed
// synchronously; never advertised to clients.
static void probeBind(struct wl_client*, void*, uint32_t, uint32_t) {}
static const struct wl_message probeReqs[] = {
    {"destroy", "", nullptr},
    {"get_item", "no", nullptr},
};
static const struct wl_message probeEvts[] = {
    {"finished", "", nullptr},
};
std::string probeGlobalCreate() {
    // Shape 1: bare interface, null bind (helper-independent baseline).
    static const struct wl_interface probeIface = {"riode_glass_probe_v1", 2, 0, nullptr, 0, nullptr};
    struct wl_global* g = wl_global_create(g_pCompositor->m_wlDisplay, &probeIface, 2, nullptr, nullptr);
    if (!g)
        return "bare-failed";
    wl_global_destroy(g);
    // Shape 2: helper-identical descriptor (2 methods, 1 event) + bind fn.
    static const struct wl_interface probeIface2 = {"riode_glass_probe_v2", 2, 2, probeReqs, 1, probeEvts};
    struct wl_global* h = wl_global_create(g_pCompositor->m_wlDisplay, &probeIface2, 2, nullptr, probeBind);
    if (!h)
        return "shaped-failed";
    wl_global_destroy(h);
    return "create-ok";
}
} // namespace

std::string formatStatus(eHyprCtlOutputFormat format) {
    const int regions = GlassBlur::lastFrameRegions();
    const auto items  = ItemHints::activeItems();
    int        nManagers = 0, nTotal = 0, nActive = 0;
    ItemHints::counts(nManagers, nTotal, nActive);
    std::string itemList;
    for (size_t i = 0; i < items.size() && i < 8; ++i) {
        if (i)
            itemList += " | ";
        itemList += rfmt::format("{}:{}", items[i].has_packed ? "glass" : "-", items[i].shape);
        if (!items[i].effects.empty())
            itemList += rfmt::format("+fx{}@{}", items[i].effects.size(), items[i].glass_stack_index);
    }
    if (items.size() > 8)
        itemList += rfmt::format(" (+{} more)", items.size() - 8);
    if (format == FORMAT_JSON) {
        return rfmt::format("{{\"plugin\": \"{}\", \"version\": \"{}\", \"built_for\": {{\"tag\": \"{}\", \"hash\": \"{}}}\", "
                           "\"rendering\": {}, \"radius\": {:.1f}, \"regions_last_frame\": {}, \"total_draws\": {}, \"draw_entry\": {}, \"exits\": [{}, {}, {}, {}, {}, {}], "
                           "\"items\": {}, \"items_detail\": \"{}\", \"hints_live\": {}, \"managers\": {}, \"items_total\": {}, \"items_active\": {}, "
                            "\"pass_adds\": {}, \"surface_adds\": {}, \"no_res\": {}, \"no_view\": {}, "
                            "\"no_effect\": {}, \"empty_region\": {}, \"noview_win\": {}, \"noview_layer\": {}, "
                            "\"noview_other\": {}, \"geo\": \"{}\", \"geo_last\": \"{}\", \"helper\": \"{}\", \"hints_init\": \"{}\", \"load_detail\": \"{}\", \"global_probe\": \"{}\", \"optics_draws\": {}, \"gl_errors\": {}, \"glue_init\": \"{}\", \"glue_load\": \"{}\", \"stage_programs\": {}, \"stage_failures\": {}}}\n",
                           PLUGIN_NAME, PLUGIN_VERSION, BUILT_FOR_TAG, BUILT_FOR_HASH, GlassBlur::active(),
                           GlassBlur::radius(), regions, GlassBlur::totalDraws(), GlassBlur::drawEntry(),
                           GlassBlur::exitStage(0), GlassBlur::exitStage(1), GlassBlur::exitStage(2),
                           GlassBlur::exitStage(3), GlassBlur::exitStage(4), GlassBlur::exitStage(5), items.size(), itemList,
                           ItemHints::live(), nManagers, nTotal, nActive,
                           GlassBlur::addCalls(),
                           GlassBlur::surfaceAdds(), GlassBlur::dbgNoRes(), GlassBlur::dbgNoView(),
                            GlassBlur::dbgNoEffect(), GlassBlur::dbgEmptyRegion(), GlassBlur::dbgNoViewWin(),
                            GlassBlur::dbgNoViewLayer(), GlassBlur::dbgNoViewOther(), GlassBlur::geoSnapshot(), GlassBlur::geoLastSnapshot(),
                            ItemHints::debugState(), ItemHints::initState(), ItemHints::loadDetail(), probeGlobalCreate(), GlassOptics::opticsDraws(), GlassBlur::glErrors(),
                            ShaderGlue::initState(), ShaderGlue::loadDetail(), ShaderGlue::stagePrograms(), ShaderGlue::stageFailures());
    }
    return rfmt::format(
        "{} {} (built for Hyprland {} {}): {} (radius {:.1f}px, {} region(s) last frame, {} draws, {} adds/{} surface, glue {})\n",
        PLUGIN_NAME, PLUGIN_VERSION, BUILT_FOR_TAG, BUILT_FOR_HASH,
        GlassBlur::active() ? "plugin blur active" : "rendering disabled (hook not installed)", GlassBlur::radius(), regions,
        GlassBlur::totalDraws(), GlassBlur::addCalls(), GlassBlur::surfaceAdds(), ShaderGlue::initState());
}

void registerCommand(HANDLE handle) {
    s_command = HyprlandAPI::registerHyprCtlCommand(handle, SHyprCtlCommand{
                                                               .name = "riode",
                                                               .exact = false,
                                                               .fn =
                                                                   [](eHyprCtlOutputFormat format, std::string request) -> std::string {
                                                                       std::string_view rest{request};
                                                                       constexpr std::string_view PREFIX = "riode";
                                                                       if (rest.starts_with(PREFIX))
                                                                           rest.remove_prefix(PREFIX.size());
                                                                       while (!rest.empty() && rest.front() == ' ')
                                                                           rest.remove_prefix(1);
                                           if (rest == "status")
                                               return formatStatus(format);
                                           if (rest.starts_with("set radius")) {
                                               try {
                                                   const float r = std::stof(std::string(rest.substr(10)));
                                                   GlassBlur::setRadius(r);
                                                   return rfmt::format("riode-glass: radius set to {:.1f}px\n", GlassBlur::radius());
                                               } catch (...) {
                                                   return "riode-glass: usage: hyprctl riode set radius <px>\n";
                                               }
                                           }
                                           return "riode-glass: usage: hyprctl riode <status|set radius <px>>  (add -j for JSON)\n";
                                                                   },
                                                           });
}

void unregisterCommand(HANDLE handle) {
    if (!s_command)
        return;
    HyprlandAPI::unregisterHyprCtlCommand(handle, s_command);
    s_command.reset();
}

} // namespace

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    const auto running = HyprlandAPI::getHyprlandVersion(handle);
    const std::string_view runningTag{running.tag};
    const std::string_view runningHash{running.hash};

    auto paused = [&](const std::string& reason, const std::string& message) {
        HyprlandAPI::addNotification(handle, rfmt::format("[{}] Paused: {}", PLUGIN_NAME, message), CHyprColor{1.0, 0.8, 0.2, 1.0}, 10000);
        return PLUGIN_DESCRIPTION_INFO{std::string(PLUGIN_NAME), rfmt::format("Paused ({}): {}", reason, message), std::string(PLUGIN_AUTHOR),
                                       std::string(PLUGIN_VERSION)};
    };

    if (!skipRequested()) {
        if (runningTag != BUILT_FOR_TAG || !runningHash.starts_with(BUILT_FOR_HASH))
            return paused("hyprland_version", rfmt::format("built for Hyprland {} {}, running {} {}", BUILT_FOR_TAG, BUILT_FOR_HASH,
                                                         runningTag, runningHash));
        // _get_hash() is Hyprland's, _get_client_hash() the one compiled into this plugin.
        if (std::string_view{__hyprland_api_get_hash()} != std::string_view{__hyprland_api_get_client_hash()})
            return paused("dependencies", "built against different Hyprland libraries than running");
    }

    for (const auto* plugin : g_pPluginSystem->getAllPlugins()) {
        if (plugin->m_name != PLUGIN_NAME)
            continue;
        using FActive = bool (*)();
        const auto active = reinterpret_cast<FActive>(dlsym(plugin->m_handle, "riode_glass_instance_active"));
        if (!active || active())
            return paused("duplicate", rfmt::format("already loaded from {}", plugin->m_path));
    }

    PHANDLE = handle;
    s_instanceActive = true;
    registerCommand(handle);

    const bool itemsLive = ItemHints::init(handle);
    const bool glueLive  = ShaderGlue::init(handle);
    const bool hookLive  = GlassBlur::install(handle);
    GlassBlur::logLine(rfmt::format("[riode-glass] loaded hook={} items={} glue={}", hookLive ? "active" : "FAILED",
                                   itemsLive ? "active" : "FAILED", glueLive ? "active" : "FAILED"));
    HyprlandAPI::addNotification(handle,
                                 rfmt::format("[{}] loaded ({}), IPC ready: `hyprctl riode status`, blur hook {}, item hints {}, shader glue {}",
                                             PLUGIN_NAME, PLUGIN_VERSION, hookLive ? "active" : "FAILED",
                                             itemsLive ? "active" : "FAILED", glueLive ? "active" : "FAILED"),
                                 hookLive && itemsLive && glueLive ? CHyprColor{0.2, 0.9, 0.4, 1.0} : CHyprColor{1.0, 0.8, 0.2, 1.0}, 5000);
    return {std::string(PLUGIN_NAME), std::string("riode desktop-backdrop glass (v2 raw-material IPC)"),
            std::string(PLUGIN_AUTHOR), std::string(PLUGIN_VERSION)};
}

APICALL EXPORT void PLUGIN_EXIT() {
    GlassBlur::logLine("[riode-glass] exiting");
    GlassBlur::uninstall(PHANDLE);
    ItemHints::shutdown(PHANDLE);
    if (PHANDLE)
        unregisterCommand(PHANDLE);
    s_instanceActive = false;
    PHANDLE = nullptr;
}

APICALL EXPORT std::string PLUGIN_API_VERSION() {
    return HYPRLAND_API_VERSION;
}
