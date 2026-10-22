//! Per-harness rendering for attaching Medulla's MCP server.
//!
//! This module owns only the CLI surfaces: Claude's inline JSON document and
//! Codex's `-c` overrides. The SDK supplies an endpoint through
//! [`McpEndpoint`], keeping grant minting and runtime configuration out of this
//! crate.
//!
//! OpenCode has no verified CLI attach path today; its wrapper uses an SSE
//! bridge, so CLI MCP registration is a follow-up when an invocation-scoped
//! mechanism is available.

use std::path::PathBuf;

use medulla_types::HarnessProvider;

/// Environment marker carried by an attached server process.
pub const ATTACHED_ENV: &str = "MEDULLA_MCP_ATTACHED";

/// Non-secret MCP endpoint details required to render a harness registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerSpec {
    /// Server key advertised to the harness.
    pub name: &'static str,
    /// Executable to start.
    pub command: PathBuf,
    /// Arguments passed to the executable.
    pub args: Vec<String>,
    /// Environment values safe to render into the harness registration.
    pub env: Vec<(String, String)>,
}

/// Supplies a session-specific endpoint to the harness attachment renderer.
pub trait McpEndpoint {
    /// Resolve the endpoint for `session`.
    fn endpoint(&self, session: &str) -> McpServerSpec;
}

/// Whether a provider's CLI has a verified invocation-scoped MCP registration.
pub fn supports_cli_attach(provider: HarnessProvider) -> bool {
    matches!(provider, HarnessProvider::Claude | HarnessProvider::Codex)
}

/// Render the inline `--mcp-config` JSON document Claude Code accepts.
pub fn claude_mcp_config(spec: &McpServerSpec) -> String {
    let mut entry = serde_json::Map::new();
    entry.insert(
        "command".to_string(),
        serde_json::Value::String(spec.command.display().to_string()),
    );
    entry.insert(
        "args".to_string(),
        serde_json::Value::Array(
            spec.args
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    if !spec.env.is_empty() {
        let env = spec
            .env
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        entry.insert("env".to_string(), serde_json::Value::Object(env));
    }
    serde_json::json!({ "mcpServers": { spec.name: serde_json::Value::Object(entry) } }).to_string()
}

/// Render Codex CLI `-c` overrides for one attached MCP server.
pub fn codex_config_args(spec: &McpServerSpec) -> Vec<String> {
    let string = |value: &str| toml_edit::Value::from(value).to_string();
    let args = spec
        .args
        .iter()
        .map(|arg| string(arg))
        .collect::<Vec<_>>()
        .join(",");
    let env = spec
        .env
        .iter()
        .filter(|(key, _)| key != ATTACHED_ENV)
        .map(|(key, value)| format!("{key}={}", string(value)))
        .collect::<Vec<_>>()
        .join(",");
    vec![
        "-c".to_string(),
        format!(
            "mcp_servers.{}.command={}",
            spec.name,
            string(&spec.command.display().to_string())
        ),
        "-c".to_string(),
        format!("mcp_servers.{}.args=[{args}]", spec.name),
        "-c".to_string(),
        format!("mcp_servers.{}.env={{{env}}}", spec.name),
        "-c".to_string(),
        format!(
            "mcp_servers.{}.env_vars=[{}]",
            spec.name,
            string(ATTACHED_ENV)
        ),
        "-c".to_string(),
        format!("mcp_servers.{}.enabled=true", spec.name),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixtureEndpoint;

    impl McpEndpoint for FixtureEndpoint {
        fn endpoint(&self, session: &str) -> McpServerSpec {
            McpServerSpec {
                name: "medulla",
                command: PathBuf::from("/opt/medulla"),
                args: vec!["mcp".to_string()],
                env: vec![
                    (ATTACHED_ENV.to_string(), format!("marker-{session}")),
                    ("MEDULLA_WORKFLOW_TOOLS".to_string(), "run".to_string()),
                ],
            }
        }
    }

    #[test]
    fn claude_json_matches_the_existing_registration_shape() {
        let spec = FixtureEndpoint.endpoint("fixture").unwrap();
        assert_eq!(
            claude_mcp_config(&spec),
            r#"{"mcpServers":{"medulla":{"command":"/opt/medulla","args":["mcp"],"env":{"MEDULLA_MCP_ATTACHED":"marker-fixture","MEDULLA_WORKFLOW_TOOLS":"run"}}}}"#
        );
    }

    #[test]
    fn codex_args_match_the_existing_registration_shape() {
        let spec = FixtureEndpoint.endpoint("fixture").unwrap();
        assert_eq!(
            codex_config_args(&spec),
            vec![
                "-c",
                "mcp_servers.medulla.command=\"/opt/medulla\"",
                "-c",
                "mcp_servers.medulla.args=[\"mcp\"]",
                "-c",
                "mcp_servers.medulla.env={MEDULLA_WORKFLOW_TOOLS=\"run\"}",
                "-c",
                "mcp_servers.medulla.env_vars=[\"MEDULLA_MCP_ATTACHED\"]",
                "-c",
                "mcp_servers.medulla.enabled=true",
            ]
        );
    }

    #[test]
    fn only_verified_cli_harnesses_support_attach() {
        assert!(supports_cli_attach(HarnessProvider::Claude));
        assert!(supports_cli_attach(HarnessProvider::Codex));
        assert!(!supports_cli_attach(HarnessProvider::Opencode));
    }
}
