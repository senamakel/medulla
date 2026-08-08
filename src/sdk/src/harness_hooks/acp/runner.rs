//! Local execution of observation-only hooks for ACP Codex sessions.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Map, Value};
use tokio::io::AsyncWriteExt;

use crate::harness_hooks::HookSpec;

/// Run matching `PostToolUse` hooks without allowing a hook failure to fail a turn.
pub async fn run_post_tool_use(
    hooks: &[HookSpec],
    cwd: &Path,
    session_id: &str,
    tool: &str,
    input: &Value,
) {
    for hook in hooks
        .iter()
        .filter(|hook| matches_tool(&hook.matcher, tool))
    {
        let payload = json!({
            "hook_event_name": "PostToolUse",
            "session_id": session_id,
            "cwd": cwd,
            "tool_name": tool,
            "tool_input": normalized_input(input),
        });
        let mut child = match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(hook.command())
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(
                    command = hook.command(),
                    "could not start ACP PostToolUse hook: {error}"
                );
                continue;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(payload.to_string().as_bytes()).await;
        }
        let timeout = Duration::from_secs(hook.timeout().unwrap_or(600));
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) if status.success() => {}
            Ok(Ok(status)) => tracing::warn!(
                command = hook.command(),
                ?status,
                "ACP PostToolUse hook failed"
            ),
            Ok(Err(error)) => tracing::warn!(
                command = hook.command(),
                "could not wait for ACP PostToolUse hook: {error}"
            ),
            Err(_) => tracing::warn!(command = hook.command(), "ACP PostToolUse hook timed out"),
        }
    }
}

fn matches_tool(matcher: &str, tool: &str) -> bool {
    matcher.is_empty()
        || matcher == "*"
        || Regex::new(matcher).is_ok_and(|regex| regex.is_match(tool))
}

fn normalized_input(input: &Value) -> Value {
    let Value::Object(input) = input else {
        return input.clone();
    };
    let mut normalized: Map<String, Value> = input.clone();
    if !normalized.contains_key("file_path") {
        if let Some(path) = normalized
            .get("path")
            .or_else(|| normalized.get("filePath"))
            .cloned()
        {
            normalized.insert("file_path".to_string(), path);
        }
    }
    Value::Object(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_preserves_codex_match_all_semantics() {
        assert!(matches_tool("*", "apply_patch"));
        assert!(matches_tool("^Bash$", "Bash"));
        assert!(!matches_tool("^Bash$", "apply_patch"));
    }

    #[test]
    fn normalizes_acp_path_for_codex_hook_consumers() {
        assert_eq!(
            normalized_input(&json!({"path":"a.rs"}))["file_path"],
            "a.rs"
        );
    }
}
