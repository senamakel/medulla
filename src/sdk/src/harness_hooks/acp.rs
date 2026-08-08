//! Hook delivery for the ACP transport, where Medulla never sees the harness's
//! own argv.
//!
//! Every other spawn door installs hooks by adding to the coding CLI's command
//! line. ACP dispatch has no such seam: Medulla spawns an ACP *server*
//! (`@agentclientprotocol/claude-agent-acp`, `codex-acp`, `opencode acp`) and
//! that server spawns the harness, so [`super::launch_args`]' argv has nowhere
//! to go. Until this module existed the whole transport ran with none of the
//! operator's hooks, and said so only in a `tracing::warn` nobody watching a
//! workflow would see.
//!
//! # Claude Code
//!
//! `claude-agent-acp` reads `_meta.claudeCode.options` off `session/new` and
//! `session/load` and passes it to the Claude Agent SDK, whose `settings`
//! option is documented as the equivalent of the `--settings` CLI flag and
//! accepts a settings object. So the document [`super::launch_args`] would have
//! put behind `--settings` travels as JSON-RPC instead, into the same
//! flag-settings layer, on top of the `user`, `project` and `local` sources that
//! server already loads.
//!
//! Verified end-to-end against `@agentclientprotocol/claude-agent-acp` 0.65.0:
//! a `SessionStart` and a `UserPromptSubmit` hook delivered this way both fired
//! during a complete turn.
//!
//! # Codex
//!
//! No channel — and not for want of one to deliver *through*. `codex-acp` reads
//! `CODEX_CONFIG` at startup and merges that JSON into the config overrides of
//! every `thread/start`, which is the same layer the CLI's `-c hooks=…` lands
//! in, so a hook document reaches Codex intact. It just does not run:
//! `codex app-server` 0.147 executes no hooks at all.
//!
//! Probed by driving the app-server through a complete authenticated turn with a
//! `SessionStart` and a `UserPromptSubmit` hook, delivered first as a
//! `thread/start` config override and then as `$CODEX_HOME/hooks.json` — the
//! operator's own documented channel — under both the `readOnly` and the
//! `dangerFullAccess` sandbox. Neither hook fired either way, and no
//! `hooks.state` entry was registered, in the same `$CODEX_HOME` where the CLI
//! path's injections *are* recorded.
//!
//! So there is nothing to deliver and nothing to trust: writing trust entries
//! for hooks that cannot run would leave the store claiming more than is true.
//! What this module does instead is say so, per spawn, naming the switch that
//! gets the hooks back. Recheck when Codex ships app-server hook support.
//!
//! # Attribution
//!
//! Deliberately not carried here. ACP dispatch already applies it through
//! [`crate::attribution::attribution_env`]'s `prepare-commit-msg` hook, which
//! the harness inherits with the rest of the environment; adding it to the
//! settings document as well would put the trailer on twice.

use serde_json::{json, Map, Value};

use crate::protocol::HarnessProvider;

use super::types::HooksConfig;

/// What an ACP-dispatched spawn carries so the operator's hooks run, and what it
/// could not carry.
///
/// Only one delivery channel is modelled because only one exists: the session
/// `_meta`. The rest of the type is the honest half — a spawn where nothing
/// could be installed still produces [`AcpDelivery::notes`], which is what keeps
/// "your hook is not running here" from being a silence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AcpDelivery {
    /// The `_meta` object for `session/new` and `session/load`, when the
    /// provider takes its configuration that way.
    ///
    /// A `serde_json` map rather than an `agent_client_protocol` type on
    /// purpose — that crate's `Meta` *is* this type, and spelling it this way
    /// keeps the hook vocabulary free of a transport dependency it otherwise
    /// has no use for.
    pub session_meta: Option<Map<String, Value>>,
    /// Operator-facing notes: hooks this transport could not install, and why.
    pub notes: Vec<String>,
}

impl AcpDelivery {
    /// Whether this delivery changes anything about the spawn.
    ///
    /// Notes alone do not count: a delivery that only explains why nothing was
    /// installed has nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.session_meta.is_none()
    }
}

/// Build the ACP-transport delivery installing `hooks` into `provider`.
///
/// Pure, and unlike [`super::launch_args`] it stays that way: the one thing
/// installation writes on the CLI path is Codex's trust store, and this
/// transport delivers nothing to Codex to trust.
///
/// Returns an empty delivery when no declared hook applies to `provider`.
/// Providers this transport cannot install into report that through
/// [`AcpDelivery::notes`] rather than failing or staying silent.
pub fn delivery(provider: HarnessProvider, hooks: &HooksConfig) -> AcpDelivery {
    let mut delivery = AcpDelivery {
        notes: super::hook_injection(provider, hooks).notes(),
        ..AcpDelivery::default()
    };
    let applicable = hooks.for_provider(provider);
    if applicable.is_empty() {
        return delivery;
    }
    let document = super::native::hook_document(&applicable);
    match provider {
        HarnessProvider::Claude => {
            delivery.session_meta = Some(claude_session_meta(&document));
        }
        HarnessProvider::Codex => {
            delivery.notes.push(format!(
                "{} hook(s) are not installed for this Codex session: it is dispatched over ACP, \
                 and `codex app-server` runs no hooks however they are delivered. Launch this \
                 harness without {}=acp for them to fire.",
                applicable.len(),
                crate::daemon::providers::HARNESS_PROTOCOL_ENV,
            ));
        }
        // Unreachable in practice: `HooksConfig::for_provider` filters on
        // `HookEvent::supported_by`, which is false for every event on both, so
        // `applicable` is already empty above — and those hooks are already
        // named in `notes` as dropped. Kept explicit rather than as a catch-all
        // so that adapting either provider makes this arm a compile error
        // instead of a silent no-op.
        HarnessProvider::Opencode | HarnessProvider::Openhuman => {}
    }
    delivery
}

/// The `_meta` object carrying `document` to `claude-agent-acp` as the Claude
/// Agent SDK's `settings` option.
///
/// Nested under `claudeCode.options` because that is where the ACP server looks;
/// the ACP spec reserves `_meta` for exactly this kind of implementation-specific
/// passenger, and a server that does not recognise the key ignores it.
fn claude_session_meta(document: &Value) -> Map<String, Value> {
    let mut meta = Map::new();
    meta.insert(
        "claudeCode".to_string(),
        json!({
            "options": {
                "settings": { "hooks": document },
            },
        }),
    );
    meta
}
