//! Shell behaviour that a model depends on: where the command runs, what a
//! failure looks like, and that the deadline is real.

use std::time::Duration;

use serde_json::json;
use tinyagents::harness::tool::{Tool, ToolCall};

use super::{tail, ShellTool, MAX_OUTPUT_BYTES};
use crate::agent::tools::fs::Workspace;

fn call(command: &str) -> ToolCall {
    ToolCall {
        id: "c1".to_string(),
        name: "shell".to_string(),
        arguments: json!({ "command": command }),
        invalid: None,
    }
}

fn tool(dir: &tempfile::TempDir, secs: u64) -> ShellTool {
    ShellTool::new(
        Workspace {
            root: dir.path().to_path_buf(),
        },
        Duration::from_secs(secs),
    )
}

#[tokio::test]
async fn a_command_runs_with_the_workspace_as_its_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("marker.txt"), "x").unwrap();

    let out = tool(&dir, 10).call(&(), call("ls")).await.unwrap();
    assert!(out.content.contains("marker.txt"), "{}", out.content);
    assert!(out.content.contains("exit: 0"));
}

/// A failing command is a result the model can act on, not a run-ending error.
/// Reporting it as an error would end the turn on the first failing test, which
/// is usually the moment the turn is most useful.
#[tokio::test]
async fn a_non_zero_exit_is_reported_without_failing_the_tool() {
    let dir = tempfile::tempdir().unwrap();
    let out = tool(&dir, 10)
        .call(&(), call("echo boom >&2; exit 3"))
        .await
        .unwrap();
    assert!(out.error.is_none(), "a non-zero exit is not a tool error");
    assert!(out.content.contains("exit: 3"), "{}", out.content);
    assert!(out.content.contains("boom"), "{}", out.content);
}

#[tokio::test]
async fn a_command_that_outruns_the_deadline_is_killed_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let out = tool(&dir, 1).call(&(), call("sleep 30")).await.unwrap();
    assert_eq!(out.error.as_deref(), Some("timeout"));
    assert!(out.content.contains("timed out"), "{}", out.content);
}

#[tokio::test]
async fn a_silent_command_says_so_rather_than_returning_nothing() {
    // An empty tool result reads to a model as a broken tool; naming the state
    // is what lets it move on instead of retrying.
    let dir = tempfile::tempdir().unwrap();
    let out = tool(&dir, 10).call(&(), call("true")).await.unwrap();
    assert!(out.content.contains("(no output)"), "{}", out.content);
}

#[tokio::test]
async fn a_missing_command_argument_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bad = ToolCall {
        id: "c1".to_string(),
        name: "shell".to_string(),
        arguments: json!({}),
        invalid: None,
    };
    assert!(tool(&dir, 10).call(&(), bad).await.is_err());
}

/// The tail, not the head: a command that failed says why on its last lines,
/// so keeping the front of a long log throws away the useful part.
#[test]
fn oversized_output_keeps_the_tail_and_names_the_drop() {
    let mut bytes = vec![b'a'; MAX_OUTPUT_BYTES + 100];
    bytes.extend_from_slice(b"THE-END");
    let kept = tail(&bytes);
    assert!(kept.contains("THE-END"), "the tail must survive");
    assert!(kept.contains("truncated"), "the drop must be named");
}

#[test]
fn output_within_the_cap_is_returned_verbatim() {
    assert_eq!(tail(b"short"), "short");
}
