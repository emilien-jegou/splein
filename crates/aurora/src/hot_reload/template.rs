// Single responsibility: Tier-1 template gate — decide if a file edit is
// template-only (literals vanishing into `view!`) or needs a rebuild.
//
// Dioxus rule, ported: strip every `view!`/`rsx!` body to an empty body, compare
// the remainder old-vs-new. Identical remainder means only template values
// changed (text, colors, sizes, literal props) and the IPC fast path may serve
// it. Anything else — handlers, hooks, branches, component count — needs the
// full cdylib rebuild. Pure string functions, unit-tested; never decides pixels.

use std::path::Path;
use std::sync::Mutex;

/// Registered template shapes per `view!` call site (`file:line` → shape).
///
/// `view!` emits a debug-only `note_view_template(file!(), line!(), SHAPE)` call
/// alongside its builder expansion. Shapes describe structure only (tags, attr
/// names, child kinds) — literal values are excluded, so a text/color edit
/// keeps the shape while adding a node changes it. The host compares shapes
/// across reloads: same shape + `TemplateOnly` gate = IPC fast path eligible.
static VIEW_SHAPES: Mutex<Option<std::collections::HashMap<String, String>>> =
    Mutex::new(None);

/// Records one `view!` site's structure shape. Debug-only by convention: the
/// macro emits the call under `#[cfg(debug_assertions)]` semantics via
/// `debug_assertions`-gated body (release call compiles to a flag check that
/// optimizes to nothing when the map is empty — kept cheap, never hot).
pub fn note_view_template(file: &str, line: u32, shape: &str) {
    if !cfg!(debug_assertions) {
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
/// for literal slots (debug builds only; release returns `default` with no
/// lookup). The dev loop extracts fresh literals from the edited file and
/// pushes them here via [`apply_template_delta`]; the next reload re-runs the
/// *existing* factory, which reads the new values — no rebuild, no dlopen.
///
/// Keys are `{line}:{idx}`: the `view!` invocation's source line plus the
/// literal's ordinal within that expansion. File scoping is handled by the
/// caller (only slots matching the edited file's registered shapes are pushed).
static TPL_STRINGS: Mutex<Option<std::collections::HashMap<String, String>>> =
    Mutex::new(None);
static TPL_F32: Mutex<Option<std::collections::HashMap<String, f32>>> =
    Mutex::new(None);

/// Reads a template string slot, falling back to the compiled default.
#[inline]
pub fn tpl_string(key: &str, default: &str) -> String {
    if !cfg!(debug_assertions) {
        return default.to_string();
    }
    let guard = TPL_STRINGS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .and_then(|m| m.get(key))
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

/// Reads a template f32 slot, falling back to the compiled default.
#[inline]
pub fn tpl_f32(key: &str, default: f32) -> f32 {
    if !cfg!(debug_assertions) {
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
/// slots actually updated.
pub fn apply_template_delta(strings: &[(String, String)], f32s: &[(String, f32)]) -> usize {
    let mut n = 0;
    if !strings.is_empty() {
        let mut guard = TPL_STRINGS.lock().unwrap_or_else(|e| e.into_inner());
        let map = guard.get_or_insert_with(Default::default);
        for (k, v) in strings {
            map.insert(k.clone(), v.clone());
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

/// Extracts `<Text>"lit"</Text>` / `<Text content="lit">` literals per `view!`
/// body, in source order, keyed `{line}:{idx}`.
///
/// Textual scan (not the macro parser): only handles the single-literal Text
/// forms. Anything fancier (format!, nested elements, `{expr}`) yields no slot
/// and the caller falls back to a rebuild. `line` is the 1-based line of the
/// `view!` token starting that body.
pub fn extract_view_literals(src: &str) -> Vec<TemplateSlot> {
    let mut slots = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = src[search_from..].find("view!") {
        let macro_at = search_from + rel;
        let line = src[..macro_at].bytes().filter(|&b| b == b'\n').count() + 1;
        // Find body open delimiter.
        let mut j = macro_at + 5;
        let bytes = src.as_bytes();
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
        // Balanced scan to body end (string/comment aware, mirrors the gate).
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
            break;
        }
        let body = &src[j + 1..k - 1];
        let mut idx = 0usize;
        let mut bpos = 0;
        while let Some(trel) = body[bpos..].find("<Text") {
            let t = bpos + trel;
            // Tag end (first `>` after `<Text`).
            let Some(tag_end_rel) = body[t..].find('>') else {
                break;
            };
            let tag = &body[t..t + tag_end_rel];
            // content="..." attr form.
            let mut lit: Option<String> = None;
            if let Some(cpos) = tag.find("content") {
                let after = &tag[cpos + 7..];
                if let Some(q1) = after.find('"') {
                    let rest = &after[q1 + 1..];
                    if let Some(q2) = rest.find('"') {
                        lit = Some(rest[..q2].to_string());
                    }
                }
            }
            let inner_start = t + tag_end_rel + 1;
            if lit.is_none() {
                // Child form: `"lit"</Text>`.
                let rest = &body[inner_start..];
                let trimmed = rest.trim_start();
                if trimmed.starts_with('"') {
                    let mut esc = false;
                    let chars: Vec<u8> = trimmed.bytes().collect();
                    let mut end = None;
                    for (ci, &c) in chars.iter().enumerate().skip(1) {
                        if esc {
                            esc = false;
                        } else if c == b'\\' {
                            esc = true;
                        } else if c == b'"' {
                            end = Some(ci);
                            break;
                        }
                    }
                    if let Some(e) = end {
                        lit = Some(trimmed[1..e].to_string());
                    }
                }
            }
            if let Some(s) = lit {
                slots.push(TemplateSlot {
                    key: format!("{line}:{idx}"),
                    string: Some(s),
                    number: None,
                });
                idx += 1;
            }
            bpos = inner_start;
        }
        let _ = open;
        search_from = k;
    }
    slots
}

/// Resolves extractor `{line}:{idx}` keys to full registry keys for one file.
///
/// Matches registry entries whose file path ends with `file_suffix` (the edited
/// file's basename) and whose line matches. Returns `(resolved, unresolved)`;
/// the caller falls back to a full rebuild unless `unresolved` is empty and
/// `resolved` is non-empty.
pub fn resolve_slot_keys(
    file_suffix: &str,
    slots: &[TemplateSlot],
) -> (Vec<(String, String)>, usize) {
    let guard = VIEW_SHAPES.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.clone().unwrap_or_default();
    let mut resolved = Vec::new();
    let mut unresolved = 0;
    for slot in slots {
        let Some(value) = slot.string.as_ref() else {
            continue;
        };
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
            Some(full) => resolved.push((full, value.clone())),
            None => unresolved += 1,
        }
    }
    (resolved, unresolved)
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
        let slots = extract_view_literals(src);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].key, "2:0");
        assert_eq!(slots[0].string.as_deref(), Some("hi"));
        assert_eq!(slots[1].key, "2:1");
        assert_eq!(slots[1].string.as_deref(), Some("yo"));
    }

    #[test]
    fn extractor_ignores_expr_children() {
        let src = "fn p() {\n view! { <Group><Text>{name}</Text></Group> }\n}";
        assert!(extract_view_literals(src).is_empty());
    }

    #[test]
    fn multibyte_text_never_panics_the_gate() {
        // Regression: byte stepping landed inside `—` and aborted the process.
        let old = "fn p() {\n view! { <Group><Text>\"a — b\"</Text></Group> }\n}";
        let new = "fn p() {\n view! { <Group><Text>\"a — c\"</Text></Group> }\n}";
        assert_eq!(classify_edit(old, new), EditKind::TemplateOnly);
        let slots = extract_view_literals(new);
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
        clear_template_values();
        assert_eq!(tpl_string("9:0", "dflt"), "dflt");
    }

    #[test]
    fn resolve_matches_registry_by_basename_and_line() {
        clear_view_templates();
        note_view_template("examples/text_smooth/view.rs", 10, "[Group]");
        let slots = vec![TemplateSlot {
            key: "10:0".to_string(),
            string: Some("new".to_string()),
            number: None,
        }];
        let (resolved, unresolved) = resolve_slot_keys("view.rs", &slots);
        assert_eq!(unresolved, 0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, "examples/text_smooth/view.rs:10:0");
        assert_eq!(resolved[0].1, "new");
        clear_view_templates();
        clear_template_values();
    }
}
