//! Update, backend, core-socket, router, and budget configuration types.

use super::*;

/// The `update` section: the periodic release-update check. Disabled entirely
/// by `check = false` here, or by the `MEDULLA_NO_UPDATE_CHECK=1` environment
/// variable (see [`UpdateConfig::enabled`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateConfig {
    /// Whether the background TUI update check runs. Defaults to `true`.
    #[serde(default = "d_true")]
    pub check: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        UpdateConfig { check: true }
    }
}

impl UpdateConfig {
    /// The effective on/off state: config `check` gated by the env kill-switch
    /// `MEDULLA_NO_UPDATE_CHECK` (any non-empty, non-`0` value disables it).
    pub fn enabled(&self, env: &HashMap<String, String>) -> bool {
        let killed = env
            .get("MEDULLA_NO_UPDATE_CHECK")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false);
        self.check && !killed
    }
}

/// The optional `core` section: the NDJSON `medulla-serve` orchestration socket.
///
/// When configured, the core runtime attaches to a long-lived `medulla-serve`
/// process over a unix domain socket (the `medulla-serve` protocol, plan §2.2).
/// This milestone is attach-only: the socket must already be listening. The
/// section is unix-only; on Windows a request for it degrades to the
/// backend→mock chain (see [`super::load_config`]).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreConfig {
    /// Explicit NDJSON unix socket path. When unset the socket is resolved from
    /// `$XDG_RUNTIME_DIR/medulla/serve.sock`, then `<stateDir>/serve.sock` (see
    /// [`LoadedConfig::core_socket_path`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,
}

/// Where the TUI reaches the Medulla backend HTTP API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BackendConfig {
    /// Base URL of the Medulla backend HTTP API.
    ///
    /// Not deserialized: the endpoint is pinned (see
    /// [`backend_base_url`](crate::config::backend_base_url)), so a `baseUrl`
    /// key in a config file parses without complaint and is then ignored. It is
    /// still serialized, so a written-out config shows where the binary
    /// actually points.
    #[serde(skip_deserializing, default = "d_backend_base")]
    pub base_url: String,
    /// Environment-variable name from which to resolve a bearer token.
    #[serde(default = "d_token_env")]
    pub token_env: String,
    /// Optional inline bearer token; environment-based credentials are preferred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Default for BackendConfig {
    fn default() -> Self {
        BackendConfig {
            base_url: d_backend_base(),
            token_env: d_token_env(),
            token: None,
        }
    }
}
