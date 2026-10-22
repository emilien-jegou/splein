#pragma once

// Plugin-safe formatting: snprintf-backed, deliberately NOT std::format.
//
// Why this exists
// ---------------
// Hyprland links libstdc++ statically and EXPORTS the weak std::__format
// template symbols (93 of them; e.g.
// `_ZNSt8__format19_Formatting_scannerINS_10_Sink_iterIcEEcE13_M_format_argEm`).
// A plugin built against a different libstdc++ (this host: GCC 15 vs the
// compositor's GCC 16) binds its std::format calls to the HOST's copies:
// the plugin's GCC-15 `_Scanner` hands a GCC-15 `basic_format_arg` to a
// GCC-16 `_Formatting_scanner` — mismatched layout — and the type-erased
// visitor jumps to a garbage pointer. The compositor then SEGFAULTs while
// loading the plugin (Hyprland crash report: SIGSEGV in
// `_M_format_arg(unsigned long)` under `ShaderGlue::init`).
//
// std::string / std::vector are fine: libstdc++ keeps the C++11 ABI for
// them across releases. std::format is C++20 and carries NO ABI
// guarantee — it is the one facility that must not cross the boundary.
// Hence this header: the plugin formats through snprintf + std::string
// only, and no std::format symbol ever enters the plugin.
//
// Supported specs (the whole plugin's vocabulary):
//   {}        default rendering (string / bool / integer / %g / %p)
//   {:.Nf}    fixed-point with N decimals
//   {:016x}   zero-padded lowercase hex
//   {{ }}     literal braces (JSON payloads)

#include <cctype>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <string_view>
#include <type_traits>
#include <utility>
#include <vector>

namespace rfmt {

/// One formatted argument, kept in a shape that lets a spec refine it
/// later (a float must still be a float when "{:.2f}" asks for two
/// decimals — pre-rendering it would lose the precision).
struct Val {
    enum class Kind { Str, Int, Uint, Dbl, Ptr };

    Kind        kind = Kind::Str;
    std::string text;                // default rendering
    double      dbl      = 0.0;      // numeric value (floats, ints)
    long long   sint     = 0;        // signed integer value
    unsigned long long uint = 0;     // unsigned integer value
};

inline Val arg(const std::string& v) {
    Val x;
    x.kind = Val::Kind::Str;
    x.text = v;
    return x;
}

inline Val arg(std::string_view v) {
    return arg(std::string(v));
}

inline Val arg(const char* v) {
    Val x;
    x.kind = Val::Kind::Str;
    x.text = v ? v : "(null)";
    return x;
}

inline Val arg(char* v) {
    return arg(static_cast<const char*>(v));
}

inline Val arg(bool v) {
    Val x;
    x.kind     = Val::Kind::Int;
    x.sint     = v ? 1 : 0;
    x.dbl      = v ? 1.0 : 0.0;
    x.text     = v ? "true" : "false";
    return x;
}

inline Val arg(std::nullptr_t) {
    Val x;
    x.kind = Val::Kind::Str;
    x.text = "(null)";
    return x;
}

template <typename T>
inline Val argInt(T v) {
    Val x;
    x.kind = std::is_signed_v<T> ? Val::Kind::Int : Val::Kind::Uint;
    x.sint = static_cast<long long>(v);
    x.uint = static_cast<unsigned long long>(v);
    x.dbl  = static_cast<double>(v);
    x.text = std::to_string(v);
    return x;
}

inline Val arg(int v) {
    return argInt(v);
}
inline Val arg(unsigned int v) {
    return argInt(v);
}
inline Val arg(long v) {
    return argInt(v);
}
inline Val arg(unsigned long v) {
    return argInt(v);
}
inline Val arg(long long v) {
    return argInt(v);
}
inline Val arg(unsigned long long v) {
    return argInt(v);
}
inline Val arg(short v) {
    return argInt(static_cast<int>(v));
}
inline Val arg(unsigned short v) {
    return argInt(static_cast<unsigned int>(v));
}
inline Val arg(signed char v) {
    return argInt(static_cast<int>(v));
}
inline Val arg(unsigned char v) {
    return argInt(static_cast<unsigned int>(v));
}

inline Val arg(float v) {
    Val x;
    x.kind = Val::Kind::Dbl;
    x.dbl  = static_cast<double>(v);
    char buf[64];
    std::snprintf(buf, sizeof(buf), "%g", x.dbl);
    x.text = buf;
    return x;
}

inline Val arg(double v) {
    Val x;
    x.kind = Val::Kind::Dbl;
    x.dbl  = v;
    char buf[64];
    std::snprintf(buf, sizeof(buf), "%g", v);
    x.text = buf;
    return x;
}

inline Val arg(const void* v) {
    Val x;
    x.kind = Val::Kind::Ptr;
    char buf[32];
    std::snprintf(buf, sizeof(buf), "%p", v);
    x.text = buf;
    return x;
}

inline Val arg(void* v) {
    return arg(static_cast<const void*>(v));
}

/// Render one value under one spec. Unknown specs fall back to the
/// default rendering (never throw, never crash).
inline std::string render(const std::string& spec, const Val& v) {
    if (spec.empty())
        return v.text;

    // {:.Nf} — fixed point.
    if (spec[0] == '.') {
        const int prec = std::atoi(spec.c_str() + 1);
        char buf[64];
        std::snprintf(buf, sizeof(buf), "%.*f", prec, v.dbl);
        return buf;
    }

    // {:016x} — zero-padded hex (width from the leading digits).
    if (spec.back() == 'x') {
        int width = 0;
        for (std::size_t i = 0; i + 1 < spec.size() && std::isdigit(static_cast<unsigned char>(spec[i])); ++i)
            width = width * 10 + (spec[i] - '0');
        char buf[64];
        std::snprintf(buf, sizeof(buf), "%0*llx", width, v.uint);
        return buf;
    }

    return v.text;
}

/// Substitute the arguments into `fmt`. Positions are fixed (the pack
/// expansion order); only the evaluation order of the arguments is
/// unspecified, exactly like std::format.
template <typename... Args>
inline std::string format(const char* fmt, Args&&... args) {
    std::vector<Val> vals;
    vals.reserve(sizeof...(Args));
    (vals.push_back(arg(std::forward<Args>(args))), ...);

    std::string out;
    std::size_t next = 0;
    for (const char* p = fmt; *p;) {
        if (*p == '{') {
            if (p[1] == '{') {
                out += '{';
                p += 2;
                continue;
            }
            const char* close = std::strchr(p, '}');
            if (!close) {
                out += *p++;
                continue;
            }
            const std::string spec(p + 1, close);
            out += render(spec, next < vals.size() ? vals[next++] : Val{});
            p = close + 1;
            continue;
        }
        if (*p == '}' && p[1] == '}') {
            out += '}';
            p += 2;
            continue;
        }
        out += *p++;
    }
    return out;
}

} // namespace rfmt
