// Single responsibility: Tier-1 template gate — decide if a file edit is
// template-only (literals vanishing into `view!`) or needs a rebuild.
//
// Dioxus rule, ported: strip every `view!`/`rsx!` body to an empty body, compare
// the remainder old-vs-new. Identical remainder means only template values
// changed (text, colors, sizes, literal props) and the IPC fast path may serve
// it. Anything else — handlers, hooks, branches, component count — needs the
// full cdylib rebuild. Pure string functions, unit-tested; never decides pixels.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Whether the template fast-path machinery is live in this process.
///
/// The switch is *process role*, never build profile: the hot dev loop builds
/// at `--profile release`, so `debug_assertions` gating left the tables dead
/// exactly where they were needed. `DevLoop::new` arms it; shipped apps never
/// construct a dev loop, so prod pays one predictable `Relaxed` load per
/// literal (returns the compiled default, no lock, no map) — no profile split,
/// no per-frame cost that matters.
static TEMPLATE_LIVE: AtomicBool = AtomicBool::new(false);

/// Arms (`true`) or disarms (`false`) the template fast path for this process.
/// Idempotent. Called by [`crate::hot_reload::DevLoop::new`]; tests toggle it
/// directly to simulate prod (off) vs dev-loop (on) runtimes.
pub fn set_template_live(enabled: bool) {
    TEMPLATE_LIVE.store(enabled, Ordering::Relaxed);
}

/// Whether the fast path is armed. Unit-tested alongside the table behavior.
pub fn is_template_live() -> bool {
    TEMPLATE_LIVE.load(Ordering::Relaxed)
}

/// Registered template shapes per `view!` call site (`file:line` → shape).
///
/// `view!` emits a `note_view_template(file!(), line!(), SHAPE)` call
/// alongside its builder expansion (compiled in every profile; a no-op unless
/// the fast path is armed). Shapes describe structure only (tags, attr
/// names, child kinds) — literal values are excluded, so a text/color edit
/// keeps the shape while adding a node changes it. The host compares shapes
/// across reloads: same shape + `TemplateOnly` gate = IPC fast path eligible.
static VIEW_SHAPES: Mutex<Option<std::collections::HashMap<String, String>>> =
    Mutex::new(None);

/// Records one `view!` site's structure shape. No-op unless armed (see
/// [`set_template_live`]): shipped apps record nothing and allocate nothing.
pub fn note_view_template(file: &str, line: u32, shape: &str) {
    if !is_template_live() {
        return;
    }
    let key = format!("{file}:{line}");
    let mut guard = VIEW_SHAPES.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(Default::default);
    map.insert(key, shape.to_string());
}

/// Looks up the last registered shape for a call site, if any.
pub fn view_shape(file: &str, line: u32) -> Option<String> {
    let guard = VIEW_SHAPES.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref()?.get(&format!("{file}:{line}")).cloned()
}

/// Number of registered shapes, for fast-path-miss diagnostics ("N unresolved
/// against M registered" tells you instantly which side is empty).
pub fn registered_shape_count() -> usize {
    VIEW_SHAPES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Clears all registered shapes (tests).
pub fn clear_view_templates() {
    if let Ok(mut guard) = VIEW_SHAPES.lock() {
        if let Some(map) = guard.as_mut() {
            map.clear();
        }
    }
}

/// Live template values (Tier-1 IPC fast path).
///
/// `view!` emits `tpl_string(key, default)` / `tpl_f32(key, default)` lookups
/// for literal slots (compiled in every profile; without arming they return
/// `default` after one atomic load — see [`set_template_live`]). The dev loop
/// extracts fresh literals from the edited file and pushes them here via
/// [`apply_template_delta`]; the next reload re-runs the *existing* factory,
/// which reads the new values — no rebuild, no dlopen.
///
/// Keys are `{line}:{idx}`: the `view!` invocation's source line plus the
/// literal's ordinal within that expansion. File scoping is handled by the
/// caller (only slots matching the edited file's registered shapes are pushed).
///
/// `tpl_string` returns `&'static str` (never `String`): builder and prop
/// signatures take `&'static str`, so the slot must be type-identical to the
/// literal it replaces. Fresh values are leaked once at apply time — bounded,
/// explicitly dev-only cruft like the retired dylib images.
static TPL_STRINGS: Mutex<Option<std::collections::HashMap<String, &'static str>>> =
    Mutex::new(None);
static TPL_F32: Mutex<Option<std::collections::HashMap<String, f32>>> =
    Mutex::new(None);

/// Reads a template string slot, falling back to the compiled default.
///
/// Unarmed (prod): one atomic load, no lock, returns `default`.
#[inline]
pub fn tpl_string(key: &str, default: &'static str) -> &'static str {
    if !is_template_live() {
        return default;
    }
    let guard = TPL_STRINGS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .and_then(|m| m.get(key))
        .copied()
        .unwrap_or(default)
}

/// Reads a template f32 slot, falling back to the compiled default.
///
/// Unarmed (prod): one atomic load, no lock, returns `default`.
#[inline]
pub fn tpl_f32(key: &str, default: f32) -> f32 {
    if !is_template_live() {
        return default;
    }
    let guard = TPL_F32.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .and_then(|m| m.get(key))
        .copied()
        .unwrap_or(default)
}

/// Pushes fresh template values from the IPC sidecar. Returns the number of
/// slots actually updated. Strings leak once (see `tpl_string`).
pub fn apply_template_delta(strings: &[(String, String)], f32s: &[(String, f32)]) -> usize {
    let mut n = 0;
    if !strings.is_empty() {
        let mut guard = TPL_STRINGS.lock().unwrap_or_else(|e| e.into_inner());
        let map = guard.get_or_insert_with(Default::default);
        for (k, v) in strings {
            map.insert(k.clone(), Box::leak(v.clone().into_boxed_str()) as &'static str);
            n += 1;
        }
    }
    if !f32s.is_empty() {
        let mut guard = TPL_F32.lock().unwrap_or_else(|e| e.into_inner());
        let map = guard.get_or_insert_with(Default::default);
        for (k, v) in f32s {
            map.insert(k.clone(), *v);
            n += 1;
        }
    }
    n
}

/// Clears live template values (tests).
pub fn clear_template_values() {
    if let Ok(mut guard) = TPL_STRINGS.lock() {
        if let Some(map) = guard.as_mut() {
            map.clear();
        }
    }
    if let Ok(mut guard) = TPL_F32.lock() {
        if let Some(map) = guard.as_mut() {
            map.clear();
        }
    }
}

/// One extracted literal slot from a `view!` body, in expansion order.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateSlot {
    /// `{line}:{idx}` key matching the macro-emitted lookup.
    pub key: String,
    /// Fresh string value, if a string literal.
    pub string: Option<String>,
    /// Fresh f32 value, if a numeric literal.
    pub number: Option<f32>,
}

/// Whether trimmed text is a plain float literal the macro would slot
/// (mirrors `slot_literal_value`'s `LitFloat`-with-dot rule: `12.0`, `.5`,
/// `16.` — never signs, idents, calls, or int-only tokens).
fn as_slot_float(trimmed: &str) -> Option<f32> {
    let t = trimmed.trim();
    if !t.contains('.') {
        return None;
    }
    if !t
        .trim_end_matches(|c: char| c == 'f' || c == 'F' || c == '3' || c == '2' || c == '6' || c == '4')
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-')
    {
        return None;
    }
    if t.starts_with(['+', '-']) || t.is_empty() {
        return None;
    }
    // Reject anything with ident chars or multiple dots (paths, method calls,
    // ranges): slots are lone numeric tokens only.
    if t.chars().any(|c| c.is_alphabetic() && c != 'e' && c != 'E') {
        return None;
    }
    if t.bytes().filter(|&b| b == b'.').count() != 1 {
        return None;
    }
    // Suffix (`f32`) must follow digits directly, like the lexer demands.
    let core = t.trim_end_matches(|c: char| c.is_alphabetic() || c == '_');
    if core.ends_with('.') || core.starts_with('.') && core.len() == 1 {
        return None;
    }
    core.parse::<f32>().ok()
}

/// Reads one `"..."` string starting at `s` (which must start with `"`),
/// returning value + byte length. Escape-aware; `None` when unterminated.
fn read_quoted(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut out = String::new();
    let mut k = 1;
    let mut escaped = false;
    while k < bytes.len() {
        let c = bytes[k];
        if escaped {
            // Keep the escape sequence verbatim (matches `value_string`-style
            // cooked output closely enough for template refresh; the rebuild
            // path remains authoritative for exotic escapes).
            out.push('\\');
            out.push(c as char);
            escaped = false;
        } else if c == b'\\' {
            escaped = true;
        } else if c == b'"' {
            return Some((out, k + 1));
        } else {
            // Boundary-safe: push full chars, never raw bytes.
            let ch = s[k..].chars().next()?;
            out.push(ch);
            k += ch.len_utf8() - 1;
        }
        k += 1;
    }
    None
}

/// Extracts slottable literals from one `view!` body in source order.
///
/// Counts exactly what the macro slots, in the same order: plain `"..."`
/// / `{ "..." }` / float attr values, `<Text>"lit"</Text>` children, and
/// `<Text content="lit">`. Anything else (`{expr}`, closures, builder
/// chains, nested elements) consumes no slot on either side.
///
/// Returns `None` when the body contains a nested `view!` (separate slot
/// namespace the linear scan cannot attribute) — the caller rebuilds.
fn extract_body_slots(body: &str, line: usize) -> Option<Vec<TemplateSlot>> {
    let mut slots = Vec::new();
    let mut idx = 0usize;
    let bytes = body.as_bytes();
    let mut p = 0;
    let mut push_string = |s: String, idx: &mut usize, slots: &mut Vec<TemplateSlot>| {
        slots.push(TemplateSlot {
            key: format!("{line}:{idx}"),
            string: Some(s),
            number: None,
        });
        *idx += 1;
    };
    let mut push_number = |v: f32, idx: &mut usize, slots: &mut Vec<TemplateSlot>| {
        slots.push(TemplateSlot {
            key: format!("{line}:{idx}"),
            string: None,
            number: Some(v),
        });
        *idx += 1;
    };
    while p < bytes.len() {
        // Nested `view!` opens a separate slot namespace: bail to rebuild.
        if body[p..].starts_with("view!")
            && {
                let r = body[p + 5..].trim_start();
                r.starts_with('{') || r.starts_with('(') || r.starts_with('[')
            }
        {
            return None;
        }
        // `<Text ...>` child or content= form.
        if body[p..].starts_with("<Text") {
            let Some(tag_end_rel) = body[p..].find('>') else {
                break;
            };
            let tag = &body[p..p + tag_end_rel];
            // content= attr form, matched as an attr NAME (whitespace before,
            // `=` after) so values merely containing the word don't count.
            let mut lit: Option<String> = None;
            let mut csearch = 0;
            while let Some(rel) = tag[csearch..].find("content") {
                let cpos = csearch + rel;
                let before_ok =
                    cpos == 0 || tag[..cpos].chars().next_back().is_some_and(|c| c.is_whitespace());
                let after_eq = tag[cpos + 7..]
                    .trim_start()
                    .strip_prefix('=')
                    .map(|s| s.trim_start());
                let Some(after_eq) = (before_ok as bool).then_some(after_eq).flatten() else {
                    csearch = cpos + 7;
                    continue;
                };
                if after_eq.starts_with('"') {
                    if let Some((s, _)) = read_quoted(after_eq) {
                        lit = Some(s);
                    }
                    break;
                } else if after_eq.starts_with('{') {
                    let inner = after_eq[1..].trim_start();
                    if inner.starts_with('"') {
                        if let Some((s, n)) = read_quoted(inner) {
                            if inner[n..].trim_start().starts_with('}') {
                                lit = Some(s);
                            }
                        }
                    }
                    // A real content= attr with a non-literal value slots
                    // nothing (macro agrees): stop, a duplicate attr would be
                    // invalid RSX anyway.
                    break;
                } else {
                    break;
                }
            }
            let inner_start = p + tag_end_rel + 1;
            if lit.is_none() {
                let rest = body[inner_start..].trim_start();
                if rest.starts_with('"') {
                    if let Some((s, _)) = read_quoted(rest) {
                        lit = Some(s);
                    }
                }
            }
            if let Some(s) = lit {
                push_string(s, &mut idx, &mut slots);
            }
            p = inner_start;
            continue;
        }
        // Attr assignment `name = value` inside a tag: slot lone literals.
        // Detected textually: ident-char run, optional space, `=`.
        if bytes[p].is_ascii_alphabetic() || bytes[p] == b'_' {
            let mut q = p;
            while q < bytes.len() && (bytes[q].is_ascii_alphanumeric() || bytes[q] == b'_' || bytes[q] == b'-' || bytes[q] == b':') {
                q += 1;
            }
            let mut r = q;
            while r < bytes.len() && bytes[r].is_ascii_whitespace() {
                r += 1;
            }
            if r < bytes.len() && bytes[r] == b'=' && {
                // `==`, `=>`, `>=`... are not assignments.
                let before_ok = true;
                let after = if r + 1 < bytes.len() { bytes[r + 1] } else { 0 };
                before_ok && after != b'=' && after != b'>'
            } {
                let mut v = r + 1;
                while v < bytes.len() && bytes[v].is_ascii_whitespace() {
                    v += 1;
                }
                let vstr = &body[v..];
                if vstr.starts_with('"') {
                    if let Some((s, n)) = read_quoted(vstr) {
                        push_string(s, &mut idx, &mut slots);
                        p = v + n;
                        continue;
                    }
                } else if vstr.starts_with('{') {
                    // Balanced braced value: slot only a lone literal inside.
                    let mut depth = 1usize;
                    let mut k = v + 1;
                    let mut in_str = false;
                    let mut esc = false;
                    while k < bytes.len() && depth > 0 {
                        let c = bytes[k];
                        if in_str {
                            if esc {
                                esc = false;
                            } else if c == b'\\' {
                                esc = true;
                            } else if c == b'"' {
                                in_str = false;
                            }
                        } else if c == b'"' {
                            in_str = true;
                        } else if c == b'{' {
                            depth += 1;
                        } else if c == b'}' {
                            depth -= 1;
                        }
                        k += 1;
                    }
                    let inner = vstr[1..k - v - 1].trim();
                    if depth == 0 {
                        if inner.starts_with('"') {
                            if let Some((s, n)) = read_quoted(inner) {
                                if inner[n..].trim().is_empty() {
                                    push_string(s, &mut idx, &mut slots);
                                }
                            }
                        } else if let Some(f) = as_slot_float(inner) {
                            push_number(f, &mut idx, &mut slots);
                        }
                    }
                    p = k;
                    continue;
                } else {
                    // Bare value: slot a lone float (`width=96.0` is not valid
                    // RSX, but costless to accept if the macro ever allows it).
                    let mut k = v;
                    while k < bytes.len()
                        && !bytes[k].is_ascii_whitespace()
                        && !matches!(bytes[k], b',' | b'>' | b'/' | b')' | b'}' | b']')
                    {
                        k += 1;
                    }
                    if let Some(f) = as_slot_float(&body[v..k]) {
                        push_number(f, &mut idx, &mut slots);
                    }
                    p = k;
                    continue;
                }
                p = v;
                continue;
            }
        }
        // Step one full char (never split a multibyte codepoint).
        let len = body[p..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        p += len;
    }
    Some(slots)
}

/// Extracts slottable literals per `view!` body, in source order, keyed
/// `{line}:{idx}`.
///
/// `line` is the 1-based line of the `view!` token starting that body.
/// Returns `None` if any body needs the rebuild path (unterminated macro or
/// nested `view!`).
pub fn extract_view_literals(src: &str) -> Option<Vec<TemplateSlot>> {
    let mut slots = Vec::new();
    let mut search_from = 0;
    let bytes = src.as_bytes();
    while let Some(rel) = src[search_from..].find("view!") {
        let macro_at = search_from + rel;
        let line = src[..macro_at].bytes().filter(|&b| b == b'\n').count() + 1;
        let mut j = macro_at + 5;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let (open, close) = match bytes.get(j) {
            Some(b'{') => (b'{', b'}'),
            Some(b'(') => (b'(', b')'),
            Some(b'[') => (b'[', b']'),
            _ => {
                search_from = macro_at + 5;
                continue;
            }
        };
        let mut depth = 1usize;
        let mut k = j + 1;
        while k < bytes.len() && depth > 0 {
            let c = bytes[k];
            if c == b'"' {
                k += 1;
                let mut escaped = false;
                while k < bytes.len() {
                    if escaped {
                        escaped = false;
                    } else if bytes[k] == b'\\' {
                        escaped = true;
                    } else if bytes[k] == b'"' {
                        k += 1;
                        break;
                    }
                    k += 1;
                }
                continue;
            }
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
            }
            k += 1;
        }
        if depth > 0 {
            return None;
        }
        let body = &src[j + 1..k - 1];
        // Per-body numbering from 0: the macro's slot counter is per
        // `view!` expansion, keyed by that expansion's own line.
        let Some(found) = extract_body_slots(body, line) else {
            return None;
        };
        slots.extend(found);
        let _ = open;
        search_from = k;
    }
    Some(slots)
}

/// Resolved fast-path values: strings and floats keyed by full slot key.
pub struct ResolvedSlots {
    /// `(full_key, fresh_string)` pairs for [`apply_template_delta`].
    pub strings: Vec<(String, String)>,
    /// `(full_key, fresh_f32)` pairs for [`apply_template_delta`].
    pub numbers: Vec<(String, f32)>,
}

/// Resolves extractor `{line}:{idx}` keys to full registry keys for one file.
///
/// Matches registry entries whose file path ends with `file_suffix` (the edited
/// file's basename) and whose line matches. Returns resolved pairs plus an
/// unresolved count; the caller falls back to a full rebuild unless
/// `unresolved` is zero and at least one pair resolved.
pub fn resolve_slot_keys(file_suffix: &str, slots: &[TemplateSlot]) -> (ResolvedSlots, usize) {
    let guard = VIEW_SHAPES.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.clone().unwrap_or_default();
    let mut out = ResolvedSlots {
        strings: Vec::new(),
        numbers: Vec::new(),
    };
    let mut unresolved = 0;
    for slot in slots {
        let mut hit: Option<String> = None;
        for reg_key in map.keys() {
            // Registry key: `{file}:{line}`; slot needs `{file}:{line}:{idx}`.
            let Some(colon) = reg_key.rfind(':') else {
                continue;
            };
            let (file, line) = (&reg_key[..colon], &reg_key[colon + 1..]);
            if !file.ends_with(file_suffix) {
                continue;
            }
            let Some(slot_colon) = slot.key.rfind(':') else {
                continue;
            };
            if line == &slot.key[..slot_colon] {
                hit = Some(format!("{reg_key}:{}", &slot.key[slot_colon + 1..]));
                break;
            }
        }
        match hit {
            Some(full) => {
                if let Some(s) = slot.string.as_ref() {
                    out.strings.push((full, s.clone()));
                } else if let Some(n) = slot.number {
                    out.numbers.push((full, n));
                } else {
                    unresolved += 1;
                }
            }
            None => unresolved += 1,
        }
    }
    (out, unresolved)
}

/// What a file edit needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// Only template values changed: IPC fast path eligible.
    TemplateOnly,
    /// Rust logic changed (or gate inconclusive): full rebuild.
    NeedsRebuild,
}

/// Strips `view! { ... }` / `view!( ... )` bodies to `view!{}`.
///
/// Balanced-delimiter scan over the source text; unterminated macros bail out
/// by returning the input unchanged (conservative: caller then sees a diff and
/// rebuilds). `view!` outside code (comments/strings) may over-strip, which
/// only ever collapses toward `TemplateOnly` when both sides strip identically
/// — and any real Rust change outside a macro body still diffs.
pub fn strip_view_bodies(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if src[i..].starts_with("view!")
            && {
                let rest = &src[i + 5..];
                let rest = rest.trim_start();
                rest.starts_with('{') || rest.starts_with('(') || rest.starts_with('[')
            }
        {
            // Find the macro body open delimiter, preserving `view!` + whitespace.
            let mut j = i + 5;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() {
                out.push_str(&src[i..]);
                break;
            }
            let (open, close) = match bytes[j] {
                b'{' => (b'{', b'}'),
                b'(' => (b'(', b')'),
                b'[' => (b'[', b']'),
                _ => {
                    out.push_str(&src[i..i + 5]);
                    i += 5;
                    continue;
                }
            };
            out.push_str("view!");
            out.push(open as char);
            out.push(close as char);
            // Skip the balanced body.
            let mut depth = 1usize;
            j += 1;
            let mut k = j;
            // Track string/char literals and comments so braces inside them
            // don't unbalance the scan.
            while k < bytes.len() && depth > 0 {
                let c = bytes[k];
                if c == b'"' || c == b'\'' {
                    let quote = c;
                    k += 1;
                    let mut escaped = false;
                    while k < bytes.len() {
                        if escaped {
                            escaped = false;
                        } else if bytes[k] == b'\\' {
                            escaped = true;
                        } else if bytes[k] == quote {
                            k += 1;
                            break;
                        }
                        // Raw strings (r#"..."#) are rare in view! bodies; a `"`.
                        // inside one ends this skip early, leaving extra depth
                        // that resolves conservatively toward NeedsRebuild.
                        k += 1;
                    }
                    continue;
                }
                if c == b'/' && k + 1 < bytes.len() && (bytes[k + 1] == b'/' || bytes[k + 1] == b'*')
                {
                    if bytes[k + 1] == b'/' {
                        k += 2;
                        while k < bytes.len() && bytes[k] != b'\n' {
                            k += 1;
                        }
                        continue;
                    } else {
                        k += 2;
                        while k + 1 < bytes.len() && !(bytes[k] == b'*' && bytes[k + 1] == b'/') {
                            k += 1;
                        }
                        k = (k + 2).min(bytes.len());
                        continue;
                    }
                }
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                }
                k += 1;
            }
            if depth > 0 {
                // Unterminated: bail conservative by keeping the rest verbatim.
                out.push_str(&src[j - 1..]);
                break;
            }
            i = k;
        } else {
            // Step one full char: byte stepping would land mid-codepoint on
            // multibyte text (e.g. `—`) and panic the next `&src[i..]` slice.
            let len = src[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            out.push_str(&src[i..i + len]);
            i += len;
        }
    }
    out
}

/// Classifies an edit from old vs new file text.
pub fn classify_edit(old: &str, new: &str) -> EditKind {
    if old == new {
        return EditKind::TemplateOnly;
    }
    if strip_view_bodies(old) == strip_view_bodies(new) {
        EditKind::TemplateOnly
    } else {
        EditKind::NeedsRebuild
    }
}

/// Classifies an edit from two files on disk. Missing/unreadable files read as
/// `NeedsRebuild` (never block the rebuild path on IO).
pub fn classify_files(old_path: &Path, new_path: &Path) -> EditKind {
    let (Ok(old), Ok(new)) = (
        std::fs::read_to_string(old_path),
        std::fs::read_to_string(new_path),
    ) else {
        return EditKind::NeedsRebuild;
    };
    classify_edit(&old, &new)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arms the fast path the way `DevLoop::new` does (tests simulate the
    /// dev-loop runtime; prod stays unarmed — see `gating_keeps_prod_clean`).
    fn arm() {
        set_template_live(true);
    }

    #[test]
    fn gating_keeps_prod_clean() {
        // Unarmed (shipped-app role): notes record nothing, lookups return
        // compiled defaults without touching the tables.
        set_template_live(false);
        assert!(!is_template_live());
        note_view_template("prod.rs", 1, "[Group]");
        assert_eq!(registered_shape_count(), 0);
        apply_template_delta(&[("1:0".to_string(), "live".to_string())], &[]);
        assert_eq!(tpl_string("1:0", "dflt"), "dflt");
        assert_eq!(tpl_f32("1:0", 9.0), 9.0);
        // Armed (dev-loop role): tables consult normally.
        set_template_live(true);
        assert!(is_template_live());
        assert_eq!(tpl_string("1:0", "dflt"), "live");
        clear_template_values();
        clear_view_templates();
    }

    #[test]
    fn armed_pipeline_registers_resolves_applies() {
        // The end-to-end path the release-gating bug broke: register (as the
        // running UI does), extract (as the dev loop does), resolve, apply,
        // read back through the slot. Would have failed unarmed.
        arm();
        clear_view_templates();
        clear_template_values();
        note_view_template("examples/row.rs", 5, "[Group]");
        let src = "a\nb\nc\nd\n view! { <Group radius={10.0}><Text>\"hi\"</Text></Group> }\n";
        let slots = extract_view_literals(src).expect("scannable");
        assert_eq!(slots.len(), 2);
        let (resolved, unresolved) = resolve_slot_keys("row.rs", &slots);
        assert_eq!(unresolved, 0);
        let n = apply_template_delta(&resolved.strings, &resolved.numbers);
        assert_eq!(n, 2);
        assert_eq!(tpl_string("examples/row.rs:5:1", "hi"), "hi");
        assert_eq!(tpl_f32("examples/row.rs:5:0", 0.0), 10.0);
        clear_template_values();
        clear_view_templates();
    }

    #[test]
    fn literal_text_change_is_template_only() {
        let old = r#"fn p() { view! { <Group><Text>"hi"</Text></Group> } }"#;
        let new = r#"fn p() { view! { <Group><Text>"hello"</Text></Group> } }"#;
        assert_eq!(classify_edit(old, new), EditKind::TemplateOnly);
    }

    #[test]
    fn handler_change_needs_rebuild() {
        let old = r#"fn p() { let a = 1; view! { <Group><Text>"hi"</Text></Group> } }"#;
        let new = r#"fn p() { let a = 2; view! { <Group><Text>"hi"</Text></Group> } }"#;
        assert_eq!(classify_edit(old, new), EditKind::NeedsRebuild);
    }

    #[test]
    fn structural_change_inside_view_still_template_only_per_conservative_gate() {
        // Documented limitation of the Dioxus-style textual gate: structure
        // inside the macro body is invisible to it. The macro-emitted
        // template shape check (next step) must catch this; the gate alone
        // treats it as template-only.
        let old = r#"fn p() { view! { <Group><Text>"hi"</Text></Group> } }"#;
        let new =
            r#"fn p() { view! { <Group><Text>"hi"</Text><Text>"yo"</Text></Group> } }"#;
        assert_eq!(classify_edit(old, new), EditKind::TemplateOnly);
    }

    #[test]
    fn unterminated_macro_falls_back_to_rebuild() {
        let old = "fn p() { view! { <Group> ";
        let new = "fn p() { view! { <Group><Text>x</Text> ";
        assert_eq!(classify_edit(old, new), EditKind::NeedsRebuild);
    }

    #[test]
    fn missing_files_need_rebuild() {
        let a = Path::new("/nonexistent-a.rs");
        let b = Path::new("/nonexistent-b.rs");
        assert_eq!(classify_files(a, b), EditKind::NeedsRebuild);
    }

    #[test]
    fn extractor_keys_text_literals_by_view_line_and_ordinal() {
        let src = "fn p() {\n view! { <Group><Text>\"hi\"</Text><Text content=\"yo\"></Text></Group> }\n}";
        let slots = extract_view_literals(src).expect("scannable");
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].key, "2:0");
        assert_eq!(slots[0].string.as_deref(), Some("hi"));
        assert_eq!(slots[1].key, "2:1");
        assert_eq!(slots[1].string.as_deref(), Some("yo"));
    }

    #[test]
    fn extractor_ignores_expr_children() {
        let src = "fn p() {\n view! { <Group><Text>{name}</Text></Group> }\n}";
        assert!(extract_view_literals(src).expect("scannable").is_empty());
    }

    #[test]
    fn extractor_slots_attr_literals_in_source_order() {
        // Mirrors the macro's expansion order: attrs in source order, then
        // children — radius, label, gap, then the Text child.
        let src = "fn p() {\n view! { <Group gap={16.0} label=\"row\"><Text>\"hi\"</Text></Group> }\n}";
        let slots = extract_view_literals(src).expect("scannable");
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].key, "2:0");
        assert_eq!(slots[0].number, Some(16.0));
        assert_eq!(slots[1].key, "2:1");
        assert_eq!(slots[1].string.as_deref(), Some("row"));
        assert_eq!(slots[2].key, "2:2");
        assert_eq!(slots[2].string.as_deref(), Some("hi"));
    }

    #[test]
    fn extractor_skips_expr_and_builder_attrs() {
        let src = "fn p() {\n view! { <Group element={group().x(x)} on_click={cb} count={n + 1} r={12.0}></Group> }\n}";
        let slots = extract_view_literals(src).expect("scannable");
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].number, Some(12.0));
    }

    #[test]
    fn extractor_rejects_plain_ints_like_the_macro() {
        // `weight=500` lexes as LitInt: the macro slots nothing, so the
        // extractor must count nothing either (else slot ordinals desync).
        let src = "fn p() {\n view! { <Group weight={500} gap={16.0}></Group> }\n}";
        let slots = extract_view_literals(src).expect("scannable");
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].number, Some(16.0));
    }

    #[test]
    fn extractor_bails_on_nested_view() {
        let src = "fn p() {\n view! { <Group>{view! { <Text>\"x\"</Text> }}</Group> }\n}";
        assert!(extract_view_literals(src).is_none());
    }

    #[test]
    fn multibyte_text_never_panics_the_gate() {
        // Regression: byte stepping landed inside `—` and aborted the process.
        let old = "fn p() {\n view! { <Group><Text>\"a — b\"</Text></Group> }\n}";
        let new = "fn p() {\n view! { <Group><Text>\"a — c\"</Text></Group> }\n}";
        assert_eq!(classify_edit(old, new), EditKind::TemplateOnly);
        let slots = extract_view_literals(new).expect("scannable");
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].string.as_deref(), Some("a — c"));
        // Multibyte outside any macro body is equally safe.
        let outside_old = "// café — déjà\nfn p() { view! { <Group/> } }";
        let outside_new = "// café — vu\nfn p() { view! { <Group/> } }";
        assert_eq!(
            classify_edit(outside_old, outside_new),
            EditKind::NeedsRebuild
        );
    }

    #[test]
    fn value_table_round_trips_with_default_fallback() {
        arm();
        clear_template_values();
        assert_eq!(tpl_string("9:0", "dflt"), "dflt");
        assert_eq!(tpl_f32("9:1", 1.5), 1.5);
        let n = apply_template_delta(
            &[("9:0".to_string(), "fresh".to_string())],
            &[("9:1".to_string(), 2.5)],
        );
        assert_eq!(n, 2);
        assert_eq!(tpl_string("9:0", "dflt"), "fresh");
        assert_eq!(tpl_f32("9:1", 1.5), 2.5);
        // Types stay `&'static str`: the slot is assignable where the literal was.
        let s: &'static str = tpl_string("9:0", "dflt");
        assert_eq!(s, "fresh");
        clear_template_values();
        assert_eq!(tpl_string("9:0", "dflt"), "dflt");
    }

    #[test]
    fn resolve_matches_registry_by_basename_and_line() {
        arm();
        clear_view_templates();
        note_view_template("examples/text_smooth/view.rs", 10, "[Group]");
        let slots = vec![TemplateSlot {
            key: "10:0".to_string(),
            string: Some("new".to_string()),
            number: None,
        }];
        let (resolved, unresolved) = resolve_slot_keys("view.rs", &slots);
        assert_eq!(unresolved, 0);
        assert_eq!(resolved.strings.len(), 1);
        assert_eq!(resolved.strings[0].0, "examples/text_smooth/view.rs:10:0");
        assert_eq!(resolved.strings[0].1, "new");
        clear_view_templates();
        clear_template_values();
    }
}
