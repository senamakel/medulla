//! Cached provider and customization facts used by the Harnesses Settings page.

/// A provider reported by `medulla-providers` discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetectedProvider {
    /// Human-facing provider name.
    pub(crate) name: String,
    /// Version reported by the collector, when available.
    pub(crate) version: Option<String>,
}

/// Facts collected before rendering the Harnesses page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HarnessesView {
    /// Detected providers and versions, if a provider scan has completed.
    pub(crate) providers: Vec<DetectedProvider>,
    /// Whether the provider scan completed, including an empty result.
    pub(crate) detection_complete: bool,
    /// Tier-to-model routing rows.
    pub(crate) routing: Vec<(String, String)>,
    /// Provider budget descriptions.
    pub(crate) budgets: Vec<(String, String)>,
    /// Facts from the effective configuration, not claims about live sessions.
    pub(crate) configuration: Vec<String>,
    /// Installed or sampled machine facts supplied by the collector.
    pub(crate) live_facts: Vec<String>,
}
