//! Unit tests for the OpenHuman hook adapter (matcher filtering, exit-status
//! semantics, and timeout bounding). Registration itself is exercised only by
//! boot coverage — `register_embedder_tool_hook` mutates a process-global
//! singleton that cannot be torn down between tests.

use openhuman_core::openhuman::agent::hooks::{ToolHookContext, ToolHookEvent};

use super::hooks::*;
use crate::harness_hooks::{HookEvent, HookHandler, HookSpec};
use crate::protocol::HarnessProvider;

fn spec(command: &str) -> HookSpec {
    HookSpec {
        event: HookEvent::PreToolUse,
        matcher: "*".into(),
        handler: HookHandler::Command {
            command: command.into(),
            timeout: None,
        },
        harnesses: vec![HarnessProvider::Openhuman],
        label: None,
        builtin: false,
    }
}

fn spec_with(command: &str, matcher: &str, timeout: Option<u64>) -> HookSpec {
    let mut hook = spec(command);
    hook.matcher = matcher.into();
    hook.handler = HookHandler::Command {
        command: command.into(),
        timeout,
    };
    hook
}

fn tool_context(tool_name: &str) -> ToolHookContext {
    ToolHookContext {
        event: ToolHookEvent::PreToolUse,
        call_id: "call-1".into(),
        tool_name: tool_name.into(),
        arguments: serde_json::json!({}),
        success: None,
        duration_ms: None,
    }
}

#[test]
fn a_selective_matcher_runs_only_for_the_tool_it_names() {
    assert!(matcher_selects("Edit", "Edit"));
    assert!(!matcher_selects("Edit", "Write"));
    assert!(!matcher_selects("Edit", "Bash"));
}

#[test]
fn a_pipe_matcher_matches_any_alternation() {
    assert!(matcher_selects("Edit|Write", "Edit"));
    assert!(matcher_selects("Edit|Write", "Write"));
    assert!(!matcher_selects("Edit|Write", "Bash"));
}

#[test]
fn the_wildcard_matcher_selects_every_tool() {
    assert!(matcher_selects("*", "Edit"));
    assert!(matcher_selects("*", "Read"));
}

#[tokio::test]
async fn a_non_matching_matcher_skips_the_command_entirely() {
    // An "Edit"-scoped command that always fails must not run for a "Bash" tool:
    // skipping it is what honouring the matcher means.
    let hooks = [spec_with("exit 1", "Edit", None)];
    run_hook_commands(&hooks, &tool_context("Bash"))
        .await
        .expect("matcher must skip the command");
}

#[tokio::test]
async fn a_matching_pre_hook_passes_and_a_failing_one_vetoes() {
    run_command(&spec("exit 0"), b"{}", true)
        .await
        .expect("a successful pre-hook passes");
    let err = run_command(&spec("exit 3"), b"{}", true)
        .await
        .expect_err("a failing pre-hook vetoes the tool call");
    assert!(err.to_string().contains("exited with"));
}

#[tokio::test]
async fn a_stop_hook_that_fails_is_observational() {
    // Stop hooks observe: a non-zero exit must not fail the turn.
    run_command(&spec("exit 9"), b"{}", false)
        .await
        .expect("stop hook failure is ignored");
}

#[tokio::test]
async fn a_command_that_exceeds_its_timeout_is_killed_not_left_to_hang() {
    let started = std::time::Instant::now();
    let err = run_command(&spec_with("sleep 5", "*", Some(1)), b"{}", true)
        .await
        .expect_err("a timed-out pre-hook vetoes");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(4),
        "the command must be killed rather than left to sleep out"
    );
    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn a_timed_out_stop_hook_is_killed_without_failing_the_turn() {
    run_command(&spec_with("sleep 5", "*", Some(1)), b"{}", false)
        .await
        .expect("a timed-out stop hook is observational");
}
