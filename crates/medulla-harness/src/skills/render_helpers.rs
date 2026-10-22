//! Small escaping helpers used by the skill text renderer.

/// A double-quoted YAML scalar, safe for any one-line description.
///
/// Frontmatter is parsed before the harness ever sees the body, so a colon in a
/// workflow description must not be able to break the document.
pub(super) fn yaml_scalar(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Renders text as a single shell word.
///
/// The fallback line is a command an operator will paste, so a workflow id or
/// JSON value with shell-significant characters must not become multiple
/// arguments or change the command's meaning.
pub(super) fn shell_quote_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        && !value.is_empty()
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
