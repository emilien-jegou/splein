#pragma once

// Custom effect stages (v3 wire): the embedded WGSL→GLSL glue plus the
// per-module GL program cache.
//
// The glue cdylib (plugins/riode-shader-glue, pure Rust/naga, never
// linked into this plugin) rides inside riode-glass.so as a blob, is
// written to a memfd and dlopen'd with RTLD_NODELETE — the exact
// item-helper pattern, so a plugin reload cannot orphan it.
//
// programFor() turns one complete wire module (client-prepended effect
// ABI + the stage's apply + the glue's fs_main entry) into a linked
// CShader, reflectively bound:
//   unit 0 = previous stage image (input), unit 1 = backdrop,
//   unit 2 = sharp parent snapshot, buffer slot 0 = CustomUniforms.
// Failures (transpile, compile, link, unrecognized bindings) are
// CACHED per module hash and logged once — no-fallback: an unsupported
// stage renders no effect, loudly.

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/render/Shader.hpp>

#include <cstdint>
#include <string>

namespace ShaderGlue {

/// Post-link binding layout of one stage program (naga glsl-out names).
struct StageBindings {
    int locInput = -1;    // _group_0_binding_2_fs -> texture unit 0
    int locBackdrop = -1; // _group_0_binding_3_fs -> texture unit 1
    int locSharp = -1;    // _group_0_binding_4_fs -> texture unit 2
    int uboBlock = -1;    // CustomUniforms block   -> buffer slot 0
    // false when the module failed (cached; logged once per hash).
    bool valid = false;
};

/// One linked stage program with its cached layout.
struct StageProgram {
    SP<CShader>  shader; // null = failed
    StageBindings bindingsValue{};
};

/// Loads the embedded glue (idempotent). False = custom stages render
/// nothing; `initState()` explains why. On false the plugin
/// notification has already been posted.
bool init(HANDLE handle);
/// Machine-readable load state: "ok", "fresh", "adopted", "memfd",
/// "write", "dlopen:<err>", "symbol", "abi", "no-glue".
std::string initState();
/// Human detail: path taken + rejected ABI versions + blob size.
std::string loadDetail();

/// Cached program for one complete wire module (both success and
/// failure stick to its hash).
StageProgram programFor(const std::string& wgsl);

/// Status counters (hyprctl riode status JSON).
uint64_t stagePrograms(); // linked since load
uint64_t stageFailures(); // failed transpile/compile/link/reflection

} // namespace ShaderGlue
