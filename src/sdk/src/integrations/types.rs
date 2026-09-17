//! Serializable data types shared by local diagnostics and remote capability advertisement.

use serde::{Deserialize, Serialize};

/// A reviewed rule family used to explain a provider's attention status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionRule {
    /// Stable rule identifier, suitable for machine-readable diagnostics.
    pub id: String,
    /// Operator-facing explanation of what the rule recognizes.
    pub explanation: String,
}

/// Operations Medulla can perform for a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationCapabilities {
    /// Whether Medulla can launch a new provider session.
    pub launch: bool,
    /// Whether Medulla can send input to an active provider session.
    pub input: bool,
    /// Whether Medulla can resume an existing provider session.
    pub resume: bool,
}

/// The result of locating and version-probing one executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    /// No configured alias was found on `PATH`.
    Missing,
    /// The executable ran its version probe successfully.
    Available,
    /// An executable was found but did not complete its version probe.
    Unhealthy,
}

/// Immutable metadata for one bundled provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapter {
    /// Stable provider identifier shared with the task and remote protocols.
    pub id: &'static str,
    /// Product name shown to an operator.
    pub display_name: &'static str,
    /// Executable aliases tried in order when no explicit binary is supplied.
    pub executable_aliases: &'static [&'static str],
    /// Arguments used for the non-interactive version probe.
    pub version_probe: &'static [&'static str],
    /// Operations implemented by the adapter.
    pub capabilities: IntegrationCapabilities,
    /// Reviewed classifier rules this adapter contributes.
    pub attention_rules: &'static [(&'static str, &'static str)],
}

/// Secret-free health report for one bundled adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationReport {
    /// Stable provider identifier.
    pub id: String,
    /// Product name displayed to an operator.
    pub display_name: String,
    /// Executable selected for the probe, never an environment-variable value.
    pub executable: String,
    /// Result of locating and probing the executable.
    pub status: ProbeStatus,
    /// Normalized first line from `--version`, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Operations exposed by this adapter.
    pub capabilities: IntegrationCapabilities,
    /// Rule ids and explanations used by attention classification.
    pub attention_rules: Vec<AttentionRule>,
    /// Non-fatal problems found while diagnosing this adapter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Explicit, provider-specific next steps an operator may take.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_guidance: Vec<String>,
}
