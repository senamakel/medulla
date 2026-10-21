//! Centralized environment-variable resolution for the harness wrapper and the
//! headless daemon.
//!
//! Every knob the wrapper/daemon read from the process environment resolves
//! here, as a pure function over an injected `&HashMap<String, String>` so the
//! precedence matrix is unit-testable and identical across both call sites.
//!
//! Precedence: a per-provider key always beats the generic (`HARNESS`) key,
//! which beats the owner fallbacks / provider defaults. Within each tier the
//! `MEDULLA_*` name wins and the deprecated `TINYPLACE_*` spelling is read
//! directly behind it, so hosts configured before the rename keep working.
//!
//! `<P>` is the uppercased provider (`CODEX` / `CLAUDE` / `OPENCODE` /
//! `OPENHUMAN`).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::protocol::HarnessProvider;
#[cfg(test)]
use medulla_providers::RouterConfig;
pub use medulla_providers::{
    bin_is_overridden, model_override, provider_args, provider_bin, router_env, RouterInjection,
};

/// Default wrapper session-file poll interval (ms).
pub const DEFAULT_SESSION_POLL_MS: u64 = 500;
/// Default inbound-receive poll interval (ms).
pub const DEFAULT_RECEIVE_POLL_MS: u64 = 1_500;
/// Default status-heartbeat re-emit interval (ms).
pub const DEFAULT_STATUS_HEARTBEAT_MS: u64 = 15_000;
/// Default silence-before-idle interval (ms).
pub const DEFAULT_STATUS_IDLE_MS: u64 = 30_000;

/// The first non-blank value among `keys`, in order.
///
/// Trimming for the emptiness check means an accidentally exported whitespace
/// value does not mask a lower-precedence, usable setting.
fn first_env<'a>(env: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter(|key| !key.is_empty())
        .filter_map(|key| env.get(*key))
        .map(String::as_str)
        .find(|value| !value.trim().is_empty())
}

/// The per-provider keys for `suffix`, highest precedence first:
/// `MEDULLA_<P>_<SUFFIX>` then the deprecated `TINYPLACE_<P>_<SUFFIX>`.
///
/// Both namespaces are read because the `TINYPLACE_*` names are what deployed
/// hosts and shell profiles already set. Dropping them would not fail loudly —
/// a worker whose `TINYPLACE_HARNESS_DM_TO` stopped resolving simply runs as a
/// plain passthrough, owning nothing and serving nobody, which reads as the
/// harness being broken rather than as a config that needs renaming.
fn provider_keys(provider: HarnessProvider, suffix: &str) -> [String; 2] {
    let p = provider.as_str().to_uppercase();
    [
        format!("MEDULLA_{p}_{suffix}"),
        format!("TINYPLACE_{p}_{suffix}"),
    ]
}

/// The generic (non-provider) keys for `suffix`, highest precedence first:
/// `MEDULLA_HARNESS_<SUFFIX>` then the deprecated `TINYPLACE_HARNESS_<SUFFIX>`.
fn harness_keys(suffix: &str) -> [String; 2] {
    [
        format!("MEDULLA_HARNESS_{suffix}"),
        format!("TINYPLACE_HARNESS_{suffix}"),
    ]
}

/// The owner this session forwards envelopes to (and, by default, receives input
/// from). Order: `MEDULLA_<P>_DM_TO` > `MEDULLA_HARNESS_DM_TO` >
/// `MEDULLA_OPENHUMAN_OWNER` > `OPENHUMAN_OWNER_AGENT`, with the deprecated
/// `TINYPLACE_*` spelling of each read directly behind it.
pub fn dm_recipient(provider: HarnessProvider, env: &HashMap<String, String>) -> Option<String> {
    let provider_keys = provider_keys(provider, "DM_TO");
    let harness_keys = harness_keys("DM_TO");
    first_env(
        env,
        &[
            &provider_keys[0],
            &provider_keys[1],
            &harness_keys[0],
            &harness_keys[1],
            "MEDULLA_OPENHUMAN_OWNER",
            "TINYPLACE_OPENHUMAN_OWNER",
            "OPENHUMAN_OWNER_AGENT",
        ],
    )
    .map(str::to_string)
}

/// The peer whose inbound frames / plain DMs are injected as input. Order:
/// `MEDULLA_<P>_RECEIVE_FROM` > `MEDULLA_HARNESS_RECEIVE_FROM` (each followed by
/// its deprecated `TINYPLACE_*` spelling), then falls back to the DM recipient.
pub fn receive_from(
    provider: HarnessProvider,
    env: &HashMap<String, String>,
    recipient: Option<&str>,
) -> Option<String> {
    let provider_keys = provider_keys(provider, "RECEIVE_FROM");
    let harness_keys = harness_keys("RECEIVE_FROM");
    first_env(
        env,
        &[
            &provider_keys[0],
            &provider_keys[1],
            &harness_keys[0],
            &harness_keys[1],
        ],
    )
    .map(str::to_string)
    .or_else(|| recipient.map(str::to_string))
}

/// Inbound input is enabled unless `MEDULLA_<P>_RECEIVE` /
/// `MEDULLA_HARNESS_RECEIVE` (or their deprecated `TINYPLACE_*` spellings) is
/// set to `"0"` (per-provider beats generic).
pub fn receive_enabled(provider: HarnessProvider, env: &HashMap<String, String>) -> bool {
    let provider_keys = provider_keys(provider, "RECEIVE");
    let harness_keys = harness_keys("RECEIVE");
    for key in provider_keys.iter().chain(harness_keys.iter()) {
        if let Some(value) = env.get(key) {
            if !value.is_empty() {
                return value != "0";
            }
        }
    }
    true
}

/// Per-provider binary override keys (first non-empty wins), highest precedence
/// first. Claude also honors the legacy `TINYVERSE_CLAUDE_BIN`; the
/// `MEDULLA_*` spellings are deprecated but still read.
///
/// OpenHuman's bare `OPENHUMAN_BIN` predates the namespaced convention and is
/// kept behind the conventional pair rather than dropped: it names the
/// standalone `openhuman-core` binary a bridged wrapper or PTY session spawns,
/// which is a real process even though the *local, in-process* provider
/// ([`crate::daemon::providers::embed`]) spawns nothing. It says nothing
/// about which model an in-process turn runs on — that is
/// [`model_override`].
/// The provider's default session-transcript directory.
fn default_sessions_dir(provider: HarnessProvider) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    match provider {
        HarnessProvider::Claude => home.join(".claude").join("projects"),
        HarnessProvider::Codex => home.join(".codex").join("sessions"),
        HarnessProvider::Opencode => home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("sessions"),
        HarnessProvider::Openhuman => home.join(".openhuman"),
        // A shell writes no transcript. `HOME` stands in so the signature
        // stays total; nothing tails it.
        HarnessProvider::Shell => home,
    }
}

/// Resolve the session-transcript directory. Order:
/// `MEDULLA_<P>_SESSIONS_DIR` > (claude only) `TINYVERSE_CLAUDE_SESSIONS_DIR` >
/// `MEDULLA_HARNESS_SESSIONS_DIR` > provider default, with the deprecated
/// `TINYPLACE_*` spelling of each read directly behind it.
pub fn sessions_dir(provider: HarnessProvider, env: &HashMap<String, String>) -> PathBuf {
    let provider_dirs = provider_keys(provider, "SESSIONS_DIR");
    let harness_dirs = harness_keys("SESSIONS_DIR");
    let tinyverse = if provider == HarnessProvider::Claude {
        "TINYVERSE_CLAUDE_SESSIONS_DIR"
    } else {
        ""
    };
    first_env(
        env,
        &[
            &provider_dirs[0],
            &provider_dirs[1],
            tinyverse,
            &harness_dirs[0],
            &harness_dirs[1],
        ],
    )
    .map(PathBuf::from)
    .unwrap_or_else(|| default_sessions_dir(provider))
}

/// Parse a positive-numeric env value, falling back silently on unset,
/// non-numeric, zero, or negative values (TS `numberEnvOr` parity).
fn number_env_or(raw: Option<&str>, fallback: u64) -> u64 {
    match raw {
        Some(value) => match value.trim().parse::<i64>() {
            Ok(parsed) if parsed > 0 => parsed as u64,
            _ => fallback,
        },
        None => fallback,
    }
}

fn timing(
    provider: HarnessProvider,
    env: &HashMap<String, String>,
    suffix: &str,
    fallback: u64,
) -> u64 {
    let provider_specific = provider_keys(provider, suffix);
    let generic = harness_keys(suffix);
    number_env_or(
        first_env(
            env,
            &[
                &provider_specific[0],
                &provider_specific[1],
                &generic[0],
                &generic[1],
            ],
        ),
        fallback,
    )
}

/// `MEDULLA_<P>_SESSION_POLL_MS` / `MEDULLA_HARNESS_SESSION_POLL_MS` (500).
pub fn session_poll_ms(provider: HarnessProvider, env: &HashMap<String, String>) -> u64 {
    timing(provider, env, "SESSION_POLL_MS", DEFAULT_SESSION_POLL_MS)
}

/// `MEDULLA_<P>_RECEIVE_POLL_MS` / `MEDULLA_HARNESS_RECEIVE_POLL_MS` (1500).
pub fn receive_poll_ms(provider: HarnessProvider, env: &HashMap<String, String>) -> u64 {
    timing(provider, env, "RECEIVE_POLL_MS", DEFAULT_RECEIVE_POLL_MS)
}

/// `MEDULLA_<P>_STATUS_HEARTBEAT_MS` / `MEDULLA_HARNESS_STATUS_HEARTBEAT_MS`
/// (15000).
pub fn status_heartbeat_ms(provider: HarnessProvider, env: &HashMap<String, String>) -> u64 {
    timing(
        provider,
        env,
        "STATUS_HEARTBEAT_MS",
        DEFAULT_STATUS_HEARTBEAT_MS,
    )
}

/// `MEDULLA_<P>_STATUS_IDLE_MS` / `MEDULLA_HARNESS_STATUS_IDLE_MS` (30000).
pub fn status_idle_ms(provider: HarnessProvider, env: &HashMap<String, String>) -> u64 {
    timing(provider, env, "STATUS_IDLE_MS", DEFAULT_STATUS_IDLE_MS)
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
