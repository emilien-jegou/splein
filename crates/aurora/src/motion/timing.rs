// Single responsibility: Parsing timeline timing tokens into absolute instants.

/// Resolves a timing token against the timeline cursor and its recorded labels.
///
/// `"<"` starts with the previous entry, `"+50ms"` offsets it, `"140ms"` pins it
/// absolutely, and anything else is looked up as a recorded label.
pub(crate) fn resolve_token(token: &str, cursor: f32, labels: &[(&'static str, f32)]) -> f32 {
    if token == "<" {
        return cursor;
    }
    if let Some(milliseconds) = token.strip_suffix("ms") {
        if let Ok(value) = milliseconds.parse::<f32>() {
            let seconds = value / 1000.0;
            return if milliseconds.starts_with('+') {
                cursor + seconds
            } else {
                seconds
            };
        }
    }
    labels
        .iter()
        .find(|(name, _)| *name == token)
        .map_or(cursor, |(_, at)| *at)
}
