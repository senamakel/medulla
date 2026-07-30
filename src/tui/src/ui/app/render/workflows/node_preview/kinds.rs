//! Kind-specific presentations for workflow step configuration.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::{Map, Value};

/// Render a node according to the semantics of its kind.
pub(super) fn kind_lines(kind: &str, config: &Value) -> Vec<Line<'static>> {
    match kind {
        "code" => code_lines(
            config
                .get("source")
                .or_else(|| config.get("code"))
                .and_then(Value::as_str)
                .unwrap_or_default(),
            config
                .get("language")
                .and_then(Value::as_str)
                .unwrap_or("javascript"),
        ),
        "agent" => agent_lines(config),
        "condition" => labelled_value(
            "condition",
            config
                .get("expression")
                .or_else(|| config.get("condition"))
                .unwrap_or(&Value::Null),
        ),
        "http_request" => request_lines(config),
        "tool_call" if config.get("slug").and_then(Value::as_str) == Some("medulla:shell") => {
            let args = config.get("args").unwrap_or(&Value::Null);
            code_lines(
                args.get("script")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                args.get("language")
                    .and_then(Value::as_str)
                    .unwrap_or("shell"),
            )
        }
        "tool_call" => tool_lines(config),
        "transform" | "set" | "merge" => labelled_value("mapping", config),
        "trigger" => labelled_value(
            "trigger",
            config.get("trigger_kind").unwrap_or(&Value::Null),
        ),
        _ => labelled_value("config", config),
    }
}

/// Render executable source with a language badge and line-number gutter.
fn code_lines(source: &str, language: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" {language} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  executable source",
            Style::default().add_modifier(Modifier::DIM),
        ),
    ])];
    if source.is_empty() {
        lines.push(Line::from(Span::styled(
            "No source configured.",
            Style::default().fg(Color::Yellow),
        )));
        return lines;
    }
    let count = source.lines().count();
    let gutter = count.to_string().len();
    for (index, source_line) in source.lines().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:>gutter$} │ ", index + 1),
                Style::default().fg(Color::DarkGray),
            ),
            highlighted_source(source_line, language),
        ]));
    }
    lines
}

/// Apply restrained syntax colour without making the viewer parser-dependent.
fn highlighted_source(line: &str, language: &str) -> Span<'static> {
    let trimmed = line.trim_start();
    let style = if trimmed.starts_with('#') || trimmed.starts_with("//") {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC)
    } else if language == "python"
        && [
            "def ", "class ", "import ", "from ", "for ", "if ", "return ",
        ]
        .iter()
        .any(|word| trimmed.starts_with(word))
        || language == "javascript"
            && [
                "const ",
                "let ",
                "function ",
                "class ",
                "import ",
                "export ",
            ]
            .iter()
            .any(|word| trimmed.starts_with(word))
    {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    Span::styled(line.to_string(), style)
}

/// Render a coding-agent step as execution metadata followed by its prompt.
fn agent_lines(config: &Value) -> Vec<Line<'static>> {
    let mut lines = metadata_line(
        config,
        &["harness", "model", "provider", "requires_approval"],
    );
    let prompt = config
        .get("prompt")
        .or_else(|| config.get("instruction"))
        .or_else(|| config.get("task"))
        .unwrap_or(&Value::Null);
    lines.extend(labelled_value("prompt", prompt));
    lines
}

/// Render an HTTP request without exposing stored credential-shaped values.
fn request_lines(config: &Value) -> Vec<Line<'static>> {
    let method = config
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET");
    let url = config
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("no URL");
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" {method} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  {url}")),
    ])];
    let rest = without_keys(config, &["method", "url"]);
    if !rest.is_empty() {
        lines.extend(labelled_value("request", &Value::Object(rest)));
    }
    lines
}

/// Render a non-shell tool invocation with its slug and arguments.
fn tool_lines(config: &Value) -> Vec<Line<'static>> {
    let slug = config
        .get("slug")
        .and_then(Value::as_str)
        .unwrap_or("unknown tool");
    let mut lines = vec![Line::from(vec![
        Span::styled("tool  ", Style::default().add_modifier(Modifier::DIM)),
        Span::styled(slug.to_string(), Style::default().fg(Color::Magenta)),
    ])];
    lines.extend(labelled_value(
        "arguments",
        config.get("args").unwrap_or(&Value::Null),
    ));
    lines
}

/// Render selected scalar metadata on one compact line.
fn metadata_line(config: &Value, keys: &[&str]) -> Vec<Line<'static>> {
    let fields = keys
        .iter()
        .filter_map(|key| {
            config
                .get(*key)
                .filter(|value| !value.is_null())
                .map(|value| format!("{key}: {}", scalar(value)))
        })
        .collect::<Vec<_>>();
    if fields.is_empty() {
        Vec::new()
    } else {
        vec![Line::from(Span::styled(
            fields.join("  ·  "),
            Style::default().fg(Color::Blue),
        ))]
    }
}

/// Render a labelled scalar or pretty, recursively redacted JSON value.
pub(super) fn labelled_value(label: &str, value: &Value) -> Vec<Line<'static>> {
    let value = redact(value);
    let text = match &value {
        Value::String(text) => text.clone(),
        Value::Null => "not configured".to_string(),
        value => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    };
    let dim = Style::default().add_modifier(Modifier::DIM);
    let mut lines = vec![Line::from(Span::styled(format!("{label}  "), dim))];
    lines.extend(
        text.lines()
            .map(|line| Line::from(Span::raw(format!("  {line}")))),
    );
    lines
}

/// Copy an object except for keys already represented in its headline.
fn without_keys(value: &Value, excluded: &[&str]) -> Map<String, Value> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !excluded.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Recursively replace credential-shaped values before they reach the screen.
fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    let secret = ["token", "secret", "password", "api_key", "authorization"]
                        .iter()
                        .any(|needle| lower.contains(needle));
                    (
                        key.clone(),
                        if secret {
                            Value::String("••••".to_string())
                        } else {
                            redact(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        value => value.clone(),
    }
}

/// Render a JSON scalar without quotes for compact metadata.
fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        value => value.to_string(),
    }
}
