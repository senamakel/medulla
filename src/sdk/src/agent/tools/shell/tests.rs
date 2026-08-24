//! Shell behaviour that a model depends on: where the command runs, what a
//! failure looks like, and that the deadline is real.

use std::time::Duration;

use serde_json::json;
use tinyagents::harness::tool::{Tool, ToolCall};

use super::{BoundedTail, ShellTool, MAX_OUTPUT_BYTES};
use crate::agent::tools::fs::Workspace;

fn call(command: &str) -> ToolCall {
    ToolCall {
        id: "c1".to_string(),
        name: "shell".to_string(),
        arguments: json!({ "command": command }),
        invalid: None,
    }
}

/// A command line that fails with status 3 after writing to stderr.
///
/// Spelled per platform because the interpreter differs: `cmd /C` takes `&` as
/// its separator and rejects `;`, which POSIX shells use. Every other command in
/// this file happens to be portable — the Windows runners carry Git for Windows,
/// so `ls`, `true`, `sleep` and `yes` all resolve — but this one is shell
/// grammar, not a program, so there is nothing on PATH to save it.
fn failing_command() -> &'static str {
    if cfg!(windows) {
        "echo boom 1>&2 & exit 3"
    } else {
        "echo boom >&2; exit 3"
    }
}

/// A command that writes forever without saturating a core.
fn steady_producer() -> &'static str {
    if cfg!(windows) {
        // `timeout` is the cmd builtin sleep; `/nobreak` stops it consuming
        // keystrokes, and stdin is null here anyway.
        "for /l %i in () do @(echo x & timeout /t 1 /nobreak >nul)"
    } else {
        "while :; do echo x; sleep 0.05; done"
    }
}

/// A tool whose commands can actually find programs.
///
/// These tests are about the shell's behaviour — working directory, exit codes,
/// deadlines, output bounds — not about environment isolation, so they get this
/// process's environment and do not have to model what a given platform needs to
/// resolve `ls` or `cmd`. Isolation has one dedicated test below, which supplies
/// a narrow map on purpose.
///
/// Guessing that set was a mistake worth recording: an allowlist of `PATH` /
/// `SYSTEMROOT` / `COMSPEC` matched nothing on Windows, which spells them
/// `Path`, `SystemRoot` and `ComSpec`, and three tests failed there while
/// passing everywhere else.
fn tool(dir: &tempfile::TempDir, secs: u64) -> ShellTool {
    tool_with_env(dir, secs, std::env::vars().collect())
}

fn tool_with_env(
    dir: &tempfile::TempDir,
    secs: u64,
    env: std::collections::HashMap<String, String>,
) -> ShellTool {
    ShellTool::new(
        Workspace {
            root: dir.path().to_path_buf(),
        },
        Duration::from_secs(secs),
        env,
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
        .call(&(), call(failing_command()))
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

/// The command runs under the supplied environment and nothing else.
///
/// This is the `env_clear` half of the boundary. The other half — that the
/// supplied map has had its credentials removed — is [`super::super::env`]'s,
/// because a daemon hands down `std::env::vars()` and installing that verbatim
/// would undo the clearing entirely.
#[cfg(unix)]
#[tokio::test]
async fn the_command_environment_is_the_supplied_one_not_this_process() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MEDULLA_TEST_SECRET", "leaked-bearer");

    // Everything the interpreter needs to run, plus one value of our own, and
    // deliberately not the secret exported above.
    //
    // A wide map still detects the bug this guards. Without `env_clear` the
    // child inherits this process's environment — which now *does* hold the
    // secret — and `.envs()` merely layers the map on top, so the secret reaches
    // the child and the second assertion fires. What is being proved is that the
    // supplied map REPLACES the environment rather than extending it, and that
    // holds however wide the map is.
    let mut env: std::collections::HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| k != "MEDULLA_TEST_SECRET")
        .collect();
    env.insert("MEDULLA_TEST_VISIBLE".to_string(), "ok".to_string());
    let out = tool_with_env(&dir, 10, env)
        .call(
            &(),
            call("echo \"$MEDULLA_TEST_VISIBLE/$MEDULLA_TEST_SECRET\""),
        )
        .await
        .unwrap();

    std::env::remove_var("MEDULLA_TEST_SECRET");
    assert!(
        out.content.contains("ok/"),
        "the supplied value must be visible: {}",
        out.content
    );
    assert!(
        !out.content.contains("leaked-bearer"),
        "this process's environment must not reach the child: {}",
        out.content
    );
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
    let mut tail = BoundedTail::new();
    tail.extend(&vec![b'a'; MAX_OUTPUT_BYTES + 100]);
    tail.extend(b"THE-END");
    let kept = tail.finish();
    assert!(kept.contains("THE-END"), "the tail must survive");
    assert!(kept.contains("truncated"), "the drop must be named");
}

#[test]
fn output_within_the_cap_is_returned_verbatim() {
    let mut tail = BoundedTail::new();
    tail.extend(b"short");
    assert_eq!(tail.finish(), "short");
}

/// The cap is on memory held, not on bytes truncated after the fact: a command
/// that outproduces the cap by orders of magnitude must still cost only the
/// cap. This is what `wait_with_output` could not give — it buffered the whole
/// stream and let the truncation run afterwards, if the host survived to reach
/// it.
#[test]
fn a_flood_costs_only_the_cap() {
    let mut tail = BoundedTail::new();
    for _ in 0..200 {
        tail.extend(&vec![b'x'; MAX_OUTPUT_BYTES]);
    }
    assert_eq!(tail.buf.len(), MAX_OUTPUT_BYTES, "memory held is bounded");
    assert_eq!(tail.dropped, MAX_OUTPUT_BYTES * 199);
}

/// A command that keeps producing is still bounded by the deadline — the drain
/// loop must not keep it alive past it.
///
/// Deliberately a *paced* producer rather than `yes`. The unbounded case is
/// covered by `a_flood_costs_only_the_cap` above, which proves the ring holds
/// memory flat without spawning anything; pegging a core here would buy the same
/// assurance while perturbing every timing-sensitive test cargo happens to run
/// on a neighbouring thread.
#[tokio::test]
async fn a_command_that_keeps_producing_is_still_killed_at_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let out = tool(&dir, 1)
        .call(&(), call(steady_producer()))
        .await
        .unwrap();
    assert_eq!(out.error.as_deref(), Some("timeout"));
}
