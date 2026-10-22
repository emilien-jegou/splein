// See ShaderGlue.hpp.

#include "Format.hpp"
#include "ShaderGlue.hpp"
#include "GlassBlur.hpp"

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/render/Shader.hpp>

#include <dlfcn.h>
#include <fcntl.h>
#include <cstdlib>
#include <cstring>
#include <elf.h>
#include <link.h>
#include <sys/mman.h>
#include <unistd.h>
#include <unordered_map>
#include <vector>

namespace {

extern "C" {
extern const unsigned char riodeShaderGlueBlobStart[];
extern const unsigned char riodeShaderGlueBlobEnd[];
}

// C mirror of plugins/riode-shader-glue's exported struct (the ABI
// check guards the layout; the glue crate owns the definition).
struct SGlueApi {
    uint32_t abi_version;
    int (*transpile)(const char* wgsl, char* glslBuf, size_t glslCap, char* errBuf, size_t errCap);
};

constexpr uint32_t GLUE_ABI_WANT = 1;
constexpr const char* GLUE_API_SYMBOL = "riode_shader_glue_v1_api";

/// Resolve one exported DATA symbol in a freshly dlopen'd object by
/// walking its link map (DT_SYMTAB/DT_STRTAB/DT_GNU_HASH) instead of
/// dlsym().
///
/// Why not dlsym: in the compositor, dlsym on the memfd'd blob reported
/// "undefined symbol: riode_shader_glue_v1_api" even though the blob
/// exports it (verified offline: byte-identical blob, `nm -D` shows the
/// symbol, dlsym finds it there). The loader's name lookup is not
/// something the plugin should depend on; the dynamic section of the
/// LOADED image is unambiguous.
///
/// Returns the symbol's runtime address, or nullptr.
const void* resolveExported(void* handle, const char* name) {
    struct link_map* lm = nullptr;
    if (dlinfo(handle, RTLD_DI_LINKMAP, &lm) != 0 || !lm || !lm->l_ld)
        return nullptr;

    const Elf64_Dyn* dyn = lm->l_ld;
    const Elf64_Sym* symtab = nullptr;
    const char* strtab = nullptr;
    const uint32_t* gnuHash = nullptr;
    size_t syment = sizeof(Elf64_Sym);
    size_t strsz = 0;
    for (; dyn->d_tag != DT_NULL; ++dyn) {
        switch (dyn->d_tag) {
            case DT_SYMTAB: symtab = reinterpret_cast<const Elf64_Sym*>(dyn->d_un.d_ptr); break;
            case DT_STRTAB: strtab = reinterpret_cast<const char*>(dyn->d_un.d_ptr); break;
            case DT_STRSZ: strsz = static_cast<size_t>(dyn->d_un.d_val); break;
            case DT_SYMENT: syment = static_cast<size_t>(dyn->d_un.d_val); break;
            case DT_GNU_HASH: gnuHash = reinterpret_cast<const uint32_t*>(dyn->d_un.d_ptr); break;
            default: break;
        }
    }
    if (!symtab || !strtab || syment == 0 || !gnuHash || syment < sizeof(Elf64_Sym))
        return nullptr;

    // GNU_HASH bounds the exported range: `symoffset` is the first
    // exported index, and the highest bucket's chain ends at the last
    // one (a chain entry with LSB 1 terminates its chain).
    const uint32_t nbuckets = gnuHash[0];
    const uint32_t symoffset = gnuHash[1];
    const uint32_t bloomSize = gnuHash[2];
    const uint64_t* bloom = reinterpret_cast<const uint64_t*>(&gnuHash[4]);
    const uint32_t* buckets = reinterpret_cast<const uint32_t*>(&bloom[bloomSize]);
    const uint32_t* chain = &buckets[nbuckets];
    uint32_t last = symoffset;
    for (uint32_t i = 0; i < nbuckets; ++i) {
        if (buckets[i] < symoffset)
            continue;
        uint32_t idx = buckets[i];
        while (!(chain[idx - symoffset] & 1u))
            ++idx;
        if (idx > last)
            last = idx;
    }
    // Defensive bounds: never walk more than a sane number of symbols,
    // and never read past the string table.
    if (last < symoffset || last - symoffset > 65536u)
        return nullptr;

    for (uint32_t i = symoffset; i <= last; ++i) {
        const Elf64_Sym& sym = symtab[i];
        if (strsz && sym.st_name >= strsz)
            continue;
        if (std::strcmp(strtab + sym.st_name, name) != 0)
            continue;
        // Shared objects map vaddr 0 at the load base: runtime = base +
        // symbol value.
        return reinterpret_cast<const void*>(static_cast<uintptr_t>(lm->l_addr + sym.st_value));
    }
    return nullptr;
}

/// Name and mapped size of a loaded object (diagnostics: a size that
/// differs from the blob we wrote means the wrong file was loaded).
std::string objectInfo(void* handle) {
    struct link_map* lm = nullptr;
    if (dlinfo(handle, RTLD_DI_LINKMAP, &lm) != 0 || !lm)
        return "object=?";
    // Program headers via dlinfo (link_map's own l_phnum/l_phdr are
    // not exposed by this glibc); the table ends with a PT_NULL entry.
    const Elf64_Phdr* phdr = nullptr;
    size_t mapped = 0;
    if (dlinfo(handle, RTLD_DI_PHDR, &phdr) == 0 && phdr) {
        for (int i = 0; i < 256; ++i) {
            if (phdr[i].p_type == PT_NULL)
                break;
            if (phdr[i].p_type == PT_LOAD)
                mapped += static_cast<size_t>(phdr[i].p_memsz);
        }
    }
    return rfmt::format("object={} mapped={}", lm->l_name ? lm->l_name : "?", mapped);
}

// GLSL from a 4 KiB WGSL module (+ 1.5 KiB ABI) is a few times larger;
// 64 KiB covers any legal wire payload (helper caps WGSL at 4 KiB).
constexpr size_t GLSL_CAP = 64 * 1024;
constexpr size_t ERR_CAP = 512;

const SGlueApi* g_glue = nullptr;
std::string      g_initState = "pending";
std::string      g_loadDetail = "none";
uint64_t         g_stagePrograms = 0;
uint64_t         g_stageFailures = 0;

bool loadGlue(HANDLE handle) {
    (void)handle; // no notification from the loader: it runs inside PLUGIN_INIT
    // Fresh-first, adopt as fallback — the item helper's generation
    // discipline (RTLD_NODELETE residents can never unload, so a stale
    // generation must not serve a newer plugin while the fresh blob works).
    bool freshOk = false;
    {
        const auto blobSize = static_cast<size_t>(riodeShaderGlueBlobEnd - riodeShaderGlueBlobStart);
        if (blobSize >= 1000) {
            // Unique memfd name AND a kept-open fd: glibc's dlopen
            // de-duplicates by PATH STRING, and a closed fd lets the
            // number be recycled — the next loader would then get the
            // PREVIOUS object back (stale blob, missing symbol). Both
            // are closed off here: unique name, fd stays open.
            static unsigned g_blobSeq = 0;
            char blobName[64];
            std::snprintf(blobName, sizeof(blobName), "riode-shader-glue-%d-%u", (int)getpid(), g_blobSeq++);
            int fd = memfd_create(blobName, MFD_CLOEXEC);
            if (fd >= 0) {
                auto* data = reinterpret_cast<const char*>(riodeShaderGlueBlobStart);
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
                        // Resolve through the link map (see
                        // resolveExported): dlsym is not dependable here.
                        const auto* fresh = static_cast<const SGlueApi*>(resolveExported(h, GLUE_API_SYMBOL));
                        if (fresh) {
                            if (fresh->abi_version == GLUE_ABI_WANT) {
                                g_glue = fresh;
                                g_loadDetail = rfmt::format("fresh-blob api={} transpile={} bytes={} {}",
                                                           (const void*)g_glue, (void*)g_glue->transpile, blobSize,
                                                           objectInfo(h));
                                freshOk = true;
                            } else {
                                g_loadDetail = rfmt::format("fresh-blob-abi-mismatch={} (want {}) bytes={} {}",
                                                           fresh->abi_version, GLUE_ABI_WANT, blobSize, objectInfo(h));
                            }
                        } else {
                            const char* err = dlerror();
                            g_loadDetail = rfmt::format("fresh-symbol-missing:{} bytes={} {}", GLUE_API_SYMBOL, blobSize,
                                                       objectInfo(h));
                            if (err)
                                g_loadDetail += rfmt::format(" (dlerror: {})", err);
                        }
                    } else {
                        const char* err = dlerror();
                        g_loadDetail = rfmt::format("fresh-dlopen-failed:{}", err ? err : "?");
                    }
                } else {
                    g_loadDetail = rfmt::format("fresh-write-short={}/{}", written, blobSize);
                }
                // NOTE: the fd is intentionally NOT closed: the
                // /proc/self/fd path must stay unique for this object
                // (see the memfd name comment above). One fd per
                // plugin load is the price; the plugin loads once.
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

    if (void* adopted = dlsym(RTLD_DEFAULT, "riode_shader_glue_v1_api")) {
        g_glue = static_cast<const SGlueApi*>(adopted);
        if (g_glue && g_glue->abi_version == GLUE_ABI_WANT) {
            g_initState = "adopted";
            g_loadDetail = rfmt::format("adopted-resident api={} (fresh failed: {})", (void*)g_glue, g_loadDetail);
            return true;
        }
        g_loadDetail += rfmt::format(" | adopt-rejected-abi-{}", g_glue ? g_glue->abi_version : 9999);
        g_glue = nullptr;
    } else {
        g_loadDetail += " | adopt-no-resident";
    }

    g_initState = "no-glue";
    // No notification from here: this runs inside PLUGIN_INIT, and
    // posting a Hyprland notification that early is not worth the risk.
    // The failure stays loud — the plugin log line and the status JSON
    // (glue_init / glue_load) both name it.
    return false;
}

// FNV-1a over the complete wire module: the cache key. Success and
// failure both stick to it, so a broken stage logs once, not per frame.
uint64_t fnv1a(const std::string& s) {
    uint64_t h = 1469598103934665603ull;
    for (unsigned char c : s) {
        h ^= c;
        h *= 1099511628211ull;
    }
    return h;
}

// Own trivial passthrough vertex: matches GlassBlur's quad VAO layout
// (location 0 = pos, `proj` slot resolved through CShader's uniform
// table exactly like BLUR_VERT). naga's fragment stage reads
// gl_FragCoord only, so there are no varyings.
constexpr const char* STAGE_VERT = R"GLSL(
#version 300 es
layout(location = 0) in vec2 pos;
uniform mat3 proj;
void main() {
    vec3 p = proj * vec3(pos, 1.0);
    gl_Position = vec4(p.xy, 0.0, 1.0);
}
)GLSL";

// Per-program binding layout (draw code reads it off the StageProgram).
using SBindings = ShaderGlue::StageBindings;
std::unordered_map<GLuint, SBindings> g_bindings;

// Post-link reflection. The names are the contract between this file and
// the glue crate's pinned naga version: module-scope uniforms materialize
// as `_group_0_binding_<N>_fs` (N = WGSL binding number) and the
// CustomUniforms UBO as the program's one block. Anything unrecognized
// fails the stage — a silent misbind would sample the wrong texture
// without ever raising a GL error.
//
// glUniform* targets the CURRENT program on ES 3.0 (no
// glProgramUniform*): bind for the duration of the reflection, restore
// after — this runs mid-frame on first compile.
bool reflect(GLuint prog) {
    GLint prevProgram = 0;
    glGetIntegerv(GL_CURRENT_PROGRAM, &prevProgram);
    glUseProgram(prog);
    auto restore = [&] {
        glUseProgram(prevProgram);
    };
    SBindings b;
    GLint nUniforms = 0;
    glGetProgramiv(prog, GL_ACTIVE_UNIFORMS, &nUniforms);
    char name[256];
    for (GLint i = 0; i < nUniforms; ++i) {
        GLsizei len = 0;
        glGetActiveUniform(prog, static_cast<GLuint>(i), sizeof(name), &len, nullptr, nullptr, name);
        if (len <= 0 || len >= sizeof(name))
            continue;
        // Our own vertex uniform — not part of the naga contract.
        if (std::strcmp(name, "proj") == 0)
            continue;
        // Parse `_group_0_binding_<N>_fs`.
        int n = -1;
        if (std::strncmp(name, "_group_0_binding_", 17) == 0 && len >= 21 &&
            std::strcmp(name + len - 3, "_fs") == 0) {
            n = std::atoi(name + 17);
        }
        if (n == 1) {
            GLint  block = -1;
            const GLuint uidx = static_cast<GLuint>(i);
            const GLenum pname = GL_UNIFORM_BLOCK_INDEX;
            glGetActiveUniformsiv(prog, 1, &uidx, pname, &block);
            if (block >= 0)
                b.uboBlock = block;
        } else if (n == 2 || n == 3 || n == 4) {
            const GLint loc = glGetUniformLocation(prog, name);
            if (loc < 0) {
                restore();
                return false;
            }
            // Fixed units: 2 input, 3 backdrop, 4 sharp.
            glUniform1i(loc, n - 2);
            (n == 2 ? b.locInput : (n == 3 ? b.locBackdrop : b.locSharp)) = loc;
        } else if (std::strstr(name, "[0]") == nullptr) {
            // Active, not on the naga contract — do not guess.
            GlassBlur::logLine(rfmt::format("[riode-glass] shader-glue: unrecognized uniform `{}` — stage refused", name));
            restore();
            return false;
        }
    }
    if (b.uboBlock < 0) {
        // Block members may be absent from the active-uniform list
        // (block-scope instance names): consult the block table.
        GLint nBlocks = 0;
        glGetProgramiv(prog, GL_ACTIVE_UNIFORM_BLOCKS, &nBlocks);
        for (GLint bi = 0; bi < nBlocks; ++bi) {
            GLsizei len = 0;
            glGetActiveUniformBlockName(prog, static_cast<GLuint>(bi), sizeof(name), &len, name);
            if (len > 0 && (std::strstr(name, "CustomUniforms") || std::strstr(name, "_group_0_binding_1"))) {
                b.uboBlock = bi;
                break;
            }
        }
        if (b.uboBlock < 0 && nBlocks == 1) {
            // A custom module has structurally at most one UBO.
            b.uboBlock = 0;
        } else if (b.uboBlock < 0 && nBlocks > 1) {
            GlassBlur::logLine(
                rfmt::format("[riode-glass] shader-glue: {} UBO blocks, none CustomUniforms — stage refused", nBlocks));
            restore();
            return false;
        }
        // nBlocks == 0: apply() ignores the uniforms — legitimately unbound.
    }
    if (b.uboBlock >= 0)
        glUniformBlockBinding(prog, b.uboBlock, 0);
    if (b.locInput < 0) {
        // The entry always samples the input; an inactive input sampler
        // means the fragment stage got optimized past the ABI contract.
        GlassBlur::logLine("[riode-glass] shader-glue: input sampler inactive — stage refused");
        restore();
        return false;
    }
    b.valid = true;
    g_bindings[prog] = b;
    restore();
    return true;
}

SP<CShader> compile(const std::string& wgsl, uint64_t hash) {
    if (!g_glue)
        return nullptr;
    std::vector<char> glsl(GLSL_CAP, '\0');
    std::vector<char> err(ERR_CAP, '\0');
    if (g_glue->transpile(wgsl.c_str(), glsl.data(), glsl.size(), err.data(), err.size()) != 1) {
        GlassBlur::logLine(rfmt::format("[riode-glass] shader-glue: transpile failed (hash {:016x}): {}", hash,
                                       err.data()));
        return nullptr;
    }
    auto shader = makeShared<CShader>();
    if (!shader->createProgram(STAGE_VERT, glsl.data(), true)) {
        GlassBlur::logLine(rfmt::format("[riode-glass] shader-glue: compile failed (hash {:016x})", hash));
        return nullptr;
    }
    if (!reflect(shader->program())) {
        GlassBlur::logLine(rfmt::format("[riode-glass] shader-glue: reflection failed (hash {:016x})", hash));
        return nullptr;
    }
    GlassBlur::logLine(rfmt::format("[riode-glass] shader-glue: stage linked (hash {:016x}, program {})", hash,
                                   shader->program()));
    return shader;
}

std::unordered_map<uint64_t, ShaderGlue::StageProgram> g_cache;

} // namespace

namespace ShaderGlue {

bool init(HANDLE handle) {
    if (g_glue)
        return true;
    return loadGlue(handle);
}

std::string initState() {
    return g_initState;
}

std::string loadDetail() {
    return g_loadDetail;
}

StageProgram programFor(const std::string& wgsl) {
    const uint64_t hash = fnv1a(wgsl);
    if (auto it = g_cache.find(hash); it != g_cache.end())
        return it->second;
    SP<CShader> shader;
    if (g_glue) {
        shader = compile(wgsl, hash);
    } else {
        GlassBlur::logLine(rfmt::format("[riode-glass] shader-glue: unavailable — stage skipped (hash {:016x})", hash));
    }
    StageProgram sp;
    if (shader) {
        ++g_stagePrograms;
        auto bit = g_bindings.find(shader->program());
        if (bit != g_bindings.end())
            sp = StageProgram{shader, bit->second};
        else
            shader = nullptr; // unreachable: reflect() published the layout
    }
    if (!sp.shader)
        ++g_stageFailures;
    g_cache.emplace(hash, sp);
    return sp;
}

uint64_t stagePrograms() {
    return g_stagePrograms;
}

uint64_t stageFailures() {
    return g_stageFailures;
}

} // namespace ShaderGlue
