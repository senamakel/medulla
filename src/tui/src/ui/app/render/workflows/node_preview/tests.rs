//! Focused tests for kind-aware workflow step previews.

use ratatui::text::Line;
use serde_json::json;

use medulla::workflows::{RunRecord, RunStatus, RunStep};

use super::{kind_lines, run_lines};

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
        80,
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
        80,
    ));

    assert!(preview.contains("executable source"), "{preview}");
    assert!(preview.contains("1 │ cargo test"), "{preview}");
    assert!(preview.contains("2 │ cargo clippy"), "{preview}");
}

#[test]
fn wrapped_code_keeps_a_blank_gutter_on_continuation_lines() {
    let lines = kind_lines(
        "code",
        &json!({
            "language": "shell",
            "source": "printf '%s' \"$pr\" | jq -c --argjson result \"$roll\""
        }),
        24,
    );
    let rendered = lines
        .iter()
        .skip(1)
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert!(rendered.len() > 1, "{rendered:?}");
    assert!(rendered[0].starts_with("1 │ "), "{rendered:?}");
    assert!(
        rendered.iter().skip(1).all(|line| line.starts_with("  │ ")),
        "{rendered:?}"
    );
    assert!(lines.iter().skip(1).all(|line| line.width() <= 24));
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
        80,
    ));

    assert!(preview.contains("POST"), "{preview}");
    assert!(preview.contains("X-Trace"), "{preview}");
    assert!(preview.contains("visible"), "{preview}");
    assert!(!preview.contains("Bearer private"), "{preview}");
    assert!(!preview.contains("\"private\""), "{preview}");
    assert!(preview.contains("••••"), "{preview}");
}

#[test]
fn agent_run_detail_shows_the_resolved_prompt_and_plain_reply() {
    let run = RunRecord {
        id: "run-1".into(),
        workflow_id: "demo".into(),
        status: RunStatus::Succeeded,
        started_at: 1,
        finished_at: Some(2),
        steps: vec![RunStep {
            node_id: "agent".into(),
            status: "success".into(),
            duration_ms: 3,
            input: Some(json!("Review every changed file\nand explain the risk.")),
            output: Some(json!([
                { "json": { "text": "The change is safe.\nTests cover the edge case." } }
            ])),
            diagnostics: Vec::new(),
        }],
        pending_approvals: Vec::new(),
        error: None,
        summary: None,
        diagnosis: None,
    };

    let preview = text(run_lines(&run, "agent", true));

    assert!(preview.contains("prompt"), "{preview}");
    assert!(preview.contains("Review every changed file"), "{preview}");
    assert!(preview.contains("and explain the risk."), "{preview}");
    assert!(preview.contains("output"), "{preview}");
    assert!(preview.contains("The change is safe."), "{preview}");
    assert!(preview.contains("Tests cover the edge case."), "{preview}");
    assert!(!preview.contains("\"json\""), "{preview}");
}
