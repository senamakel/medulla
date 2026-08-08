//! The in-process adapter that translates Medulla's operator hook config into
//! OpenHuman's embedder lifecycle hooks.
//!
//! [`super::boot_with_hooks`] asks this module to register the configured
//! `PreToolUse`, `PostToolUse`, and `Stop` hooks once, before the core builds.
//! Registration is process-global and append-only — OpenHuman's
//! `register_embedder_tool_hook` / `register_embedder_post_turn_hook` push onto
//! singleton lists — so a host registers once per process, when it boots its one
//! core.
//!
//! Each [`HookSpec`] is kept whole rather than reduced to a command string so
//! two properties the operator declared survive translation: the **matcher**,
//! which scopes a tool hook to the tools it names instead of firing on every
//! one, and the **timeout**, which bounds a slow command so a stuck hook cannot
//! stall the tool call or the turn.

use std::sync::Arc;
use std::time::Duration;

use openhuman_core::openhuman::agent::hooks::{
    register_embedder_post_turn_hook, register_embedder_tool_hook, PostTurnHook, ToolHook,
    ToolHookContext, TurnContext,
};

use crate::harness_hooks::{HookEvent, HookSpec, HooksConfig};
use crate::protocol::HarnessProvider;

/// Register the configured OpenHuman lifecycle hooks in this process.
///
/// Called by [`super::boot_with_hooks`] before the core builds. Splits the
/// hooks that apply to OpenHuman by event and registers a stop hook and/or a
/// tool hook, each only when the config actually declares one — a host with no
/// hooks registers nothing.
///
/// Because registration is append-only and process-global, calling this more
/// than once in a process would install duplicate hooks; callers boot the one
/// core a process owns.
pub(crate) fn register_lifecycle_hooks(hooks: &HooksConfig) {
    let mut stop = Vec::new();
    let mut pre = Vec::new();
    let mut post = Vec::new();
    for spec in hooks.for_provider(HarnessProvider::Openhuman) {
        match spec.event {
            HookEvent::Stop => stop.push(spec.clone()),
            HookEvent::PreToolUse => pre.push(spec.clone()),
            HookEvent::PostToolUse => post.push(spec.clone()),
            _ => {}
        }
    }
    if !stop.is_empty() {
        register_embedder_post_turn_hook(Arc::new(MedullaStopHook { commands: stop }));
    }
    if !pre.is_empty() || !post.is_empty() {
        register_embedder_tool_hook(Arc::new(MedullaToolHook {
            pre_commands: pre,
            post_commands: post,
        }));
    }
}

/// Runs Medulla's command hooks when an in-process OpenHuman turn completes.
///
/// Exit status is deliberately ignored: a `Stop` hook observes, and a cleanup
/// command returning non-zero must not fail the turn. Each command is still
/// bounded by its configured timeout so a stuck one cannot stall the lifecycle.
struct MedullaStopHook {
    commands: Vec<HookSpec>,
}

#[async_trait::async_trait]
impl PostTurnHook for MedullaStopHook {
    fn name(&self) -> &str {
        "medulla-stop-hook"
    }

    async fn on_turn_complete(&self, turn: &TurnContext) -> anyhow::Result<()> {
        let payload = serde_json::json!({
            "hook_event_name": "Stop", "session_id": turn.session_id,
            "agent_id": turn.agent_id, "prompt": turn.user_message,
            "response": turn.assistant_response,
        })
        .to_string();
        for spec in &self.commands {
            run_command(spec, payload.as_bytes(), false).await?;
        }
        Ok(())
    }
}

/// Medulla's synchronous/pre and observational/post tool hook adapter.
///
/// A `PreToolUse` failure (a non-zero exit or a timeout) vetoes the tool call by
/// returning an error. `PostToolUse` is observational: its commands still run,
/// but OpenHuman's dispatcher logs any error without changing the tool's result.
/// Both run only the hooks whose matcher selects the current tool name.
struct MedullaToolHook {
    pre_commands: Vec<HookSpec>,
    post_commands: Vec<HookSpec>,
}

#[async_trait::async_trait]
impl ToolHook for MedullaToolHook {
    fn name(&self) -> &str {
        "medulla-tool-hook"
    }

    async fn before_tool(&self, context: &ToolHookContext) -> anyhow::Result<()> {
        run_hook_commands(&self.pre_commands, context).await
    }

    async fn after_tool(&self, context: &ToolHookContext) -> anyhow::Result<()> {
        // Post hooks are observational; errors are logged by OpenHuman rather
        // than retroactively turning a successful tool into a failure.
        run_hook_commands(&self.post_commands, context).await
    }
}

/// Run `specs` against a tool lifecycle with structured JSON on stdin.
///
/// Only hooks whose matcher selects `context.tool_name` run. A non-zero exit or
/// timeout is returned to the caller — for a pre-hook that vetoes the tool call,
/// for a post-hook OpenHuman logs it without changing the result.
pub(super) async fn run_hook_commands(
    specs: &[HookSpec],
    context: &ToolHookContext,
) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(context)?;
    for spec in specs {
        if !matcher_selects(&spec.matcher, &context.tool_name) {
            continue;
        }
        run_command(spec, &payload, true).await?;
    }
    Ok(())
}

/// Run one hook `spec` with `payload` on stdin, bounded by its configured
/// timeout.
///
/// `enforce_status` controls whether a non-zero exit is an error: pre-hooks veto
/// on failure, while stop hooks observe. Every command is bounded by the hook's
/// configured timeout (OpenHuman's own default when none is declared); a command
/// that exceeds it is killed so a stuck hook cannot stall the tool call or the
/// turn. When `enforce_status` is set, a timeout also vetoes, matching a
/// pre-hook's blocking semantics.
pub(super) async fn run_command(
    spec: &HookSpec,
    payload: &[u8],
    enforce_status: bool,
) -> anyhow::Result<()> {
    let mut process = shell_command(spec.command());
    let mut child = process.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(payload).await?;
    }
    let waited = match spec.timeout() {
        Some(seconds) => tokio::time::timeout(Duration::from_secs(seconds), child.wait()).await,
        None => Ok(child.wait().await),
    };
    match waited {
        Ok(Ok(status)) => {
            if enforce_status {
                anyhow::ensure!(status.success(), "hook command exited with {status}");
            }
            Ok(())
        }
        Ok(Err(err)) => Err(err.into()),
        Err(_elapsed) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            tracing::warn!(
                "[core_host] hook timed out and was killed: {}",
                spec.command()
            );
            if enforce_status {
                anyhow::bail!("hook command timed out: {}", spec.command());
            }
            Ok(())
        }
    }
}

/// A `sh -c` / `cmd /C` child for `command`, with hook JSON piped in on stdin.
fn shell_command(command: &str) -> tokio::process::Command {
    let mut process = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" });
    if cfg!(windows) {
        process.args(["/C", command]);
    } else {
        process.args(["-c", command]);
    }
    process
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    process
}

/// Whether a hook's matcher selects `tool_name`.
///
/// A matcher is one or more `|`-separated glob patterns (`Edit|Write`), with `*`
/// the match-all default. Each alternation is anchored and `*` matches any run
/// of characters — the selective matchers operators write and the wildcard
/// default, not shell super-wildcards.
pub(super) fn matcher_selects(matcher: &str, tool_name: &str) -> bool {
    matcher
        .split('|')
        .filter(|pattern| !pattern.is_empty())
        .any(|pattern| pattern_selects(pattern, tool_name))
}

/// Whether one `|`-separated glob pattern selects `tool_name`.
fn pattern_selects(pattern: &str, tool_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let mut source = String::from("\\A");
    for ch in pattern.chars() {
        match ch {
            '*' => source.push_str(".*"),
            '?' => source.push('.'),
            ch => source.push_str(&regex::escape(&ch.to_string())),
        }
    }
    source.push_str("\\z");
    regex::Regex::new(&source)
        .map(|pattern| pattern.is_match(tool_name))
        .unwrap_or(false)
}
