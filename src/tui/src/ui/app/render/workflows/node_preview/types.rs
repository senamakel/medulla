//! View data shared by workflow node preview renderers.

use medulla::config::WorkflowsConfig;

/// Effective routing defaults used to explain an authored agent node.
#[derive(Debug, Default)]
pub(super) struct AgentDefaults {
    /// Worker used when a node does not name an `agent_ref`.
    pub(super) worker: String,
    /// Harness hint sent to the selected worker.
    pub(super) harness: String,
    /// Model hint sent to the selected worker.
    pub(super) model: String,
}

impl AgentDefaults {
    /// Build human-facing effective values from workflow configuration.
    pub(super) fn from_config(config: &WorkflowsConfig) -> Self {
        Self {
            worker: config.default_worker.clone(),
            harness: config
                .default_provider
                .as_ref()
                .map(|provider| provider.display_name().to_string())
                .unwrap_or_else(|| "worker default".to_string()),
            model: if config.default_model.is_empty() {
                "worker default".to_string()
            } else {
                config.default_model.clone()
            },
        }
    }
}
