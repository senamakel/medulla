//! Focused tests for kind-aware workflow step previews.

use ratatui::text::Line;
use serde_json::json;

use super::kind_lines;

/// Flatten styled lines into the text an operator reads.
fn text(lines: Vec<Line<'static>>) -> String {
    lines
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn code_steps_have_a_language_badge_and_numbered_source() {
    let preview = text(kind_lines(
        "code",
        &json!({
            "language": "python",
            "source": "def greet(name):\n    return f\"hi {name}\""
        }),
    ));

    assert!(preview.contains("python"), "{preview}");
    assert!(preview.contains("1 │ def greet(name):"), "{preview}");
    assert!(preview.contains("2 │     return"), "{preview}");
}

#[test]
fn shell_tool_steps_use_the_same_code_viewer() {
    let preview = text(kind_lines(
        "tool_call",
        &json!({
            "slug": "medulla:shell",
            "args": { "language": "shell", "script": "cargo test\ncargo clippy" }
        }),
    ));

    assert!(preview.contains("executable source"), "{preview}");
    assert!(preview.contains("1 │ cargo test"), "{preview}");
    assert!(preview.contains("2 │ cargo clippy"), "{preview}");
}

#[test]
fn generic_detail_redacts_credential_shaped_fields_recursively() {
    let preview = text(kind_lines(
        "http_request",
        &json!({
            "method": "POST",
            "url": "https://example.test",
            "headers": {
                "Authorization": "Bearer private",
                "X-Trace": "visible"
            },
            "api_key": "private"
        }),
    ));

    assert!(preview.contains("POST"), "{preview}");
    assert!(preview.contains("X-Trace"), "{preview}");
    assert!(preview.contains("visible"), "{preview}");
    assert!(!preview.contains("Bearer private"), "{preview}");
    assert!(!preview.contains("\"private\""), "{preview}");
    assert!(preview.contains("••••"), "{preview}");
}
