// Responsibility: Parses Makefile-style compiler depinfo (.d) files to extract exact source file closures.

use std::path::PathBuf;

/// Parses depinfo file text into existing filesystem paths.
pub fn parse_depinfo(text: &str) -> Vec<PathBuf> {
    let collapsed = text.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut files = Vec::new();
    let mut token = String::new();
    let mut chars = collapsed.chars().peekable();

    let flush = |token: &mut String, files: &mut Vec<PathBuf>| {
        if token.is_empty() || token == "\\" {
            token.clear();
            return;
        }
        if !token.ends_with(':') {
            let path = PathBuf::from(token.as_str());
            if path.exists() && !files.contains(&path) {
                files.push(path);
            }
        }
        token.clear();
    };

    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some(&next) if next != '\r' && next != '\n' => {
                    token.push(next);
                    chars.next();
                }
                _ => flush(&mut token, &mut files),
            },
            c if c.is_whitespace() => flush(&mut token, &mut files),
            _ => token.push(c),
        }
    }

    flush(&mut token, &mut files);
    files
}
