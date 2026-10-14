// Single responsibility: Tier-1 template gate — decide if a file edit is
// template-only (literals vanishing into `view!`) or needs a rebuild.
//
// Dioxus rule, ported: strip every `view!`/`rsx!` body to an empty body, compare
// the remainder old-vs-new. Identical remainder means only template values
// changed (text, colors, sizes, literal props) and the IPC fast path may serve
// it. Anything else — handlers, hooks, branches, component count — needs the
// full cdylib rebuild. Pure string functions, unit-tested; never decides pixels.

use std::path::Path;

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
            out.push(bytes[i] as char);
            i += 1;
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
}
