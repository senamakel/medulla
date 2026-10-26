//! A process command line as the signal prompt shows it, with secrets redacted.

use super::*;

pub(super) fn display_command(process: &ProcessSnapshot) -> String {
    let mut rendered = Vec::new();
    let mut redact_next = false;
    for argument in process
        .cmd
        .iter()
        .filter(|arg| !is_environment_assignment(arg))
    {
        if redact_next {
            redact_next = false;
            continue;
        }
        if let Some((key, _)) = argument.split_once('=') {
            if argument.starts_with('-') && is_secret_name(key) {
                rendered.push(format!("{key}=[REDACTED]"));
                continue;
            }
        }
        if argument.starts_with('-') && is_secret_name(argument) {
            rendered.push(format!("{argument}=[REDACTED]"));
            redact_next = !argument.contains('=');
        } else {
            rendered.push(argument.clone());
        }
    }
    if rendered.is_empty() {
        rendered.push(
            process
                .exe
                .as_deref()
                .map(std::path::Path::new)
                .and_then(std::path::Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("unavailable")
                .to_owned(),
        );
    }
    let command = rendered.join(" ");
    const LIMIT: usize = 72;
    if command.len() <= LIMIT {
        return command;
    }
    let mut end = LIMIT.saturating_sub(1);
    while !command.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &command[..end])
}

pub(super) fn is_environment_assignment(argument: &str) -> bool {
    let Some((name, _)) = argument.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && !argument.starts_with('-')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub(super) fn is_secret_name(name: &str) -> bool {
    let name = name.trim_start_matches('-').to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "passwd",
        "api-key",
        "apikey",
        "auth",
        "credential",
    ]
    .iter()
    .any(|secret| name.contains(secret))
}
