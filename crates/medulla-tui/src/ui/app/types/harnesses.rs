//! Cached provider and customization facts used by the Harnesses Settings page.

/// A provider reported by `medulla-providers` discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedProvider {
    /// Human-facing provider name.
    pub name: String,
    /// Version reported by the collector, when available.
    pub version: Option<String>,
}

/// Facts collected before rendering the Harnesses page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HarnessesView {
    /// Detected providers and versions, if a provider scan has completed.
    pub providers: Vec<DetectedProvider>,
    /// Whether the provider scan completed, including an empty result.
    pub detection_complete: bool,
    /// Tier-to-model routing rows.
    pub routing: Vec<(String, String)>,
    /// Provider budget descriptions.
    pub budgets: Vec<(String, String)>,
    /// Facts from the effective configuration, not claims about live sessions.
    pub configuration: Vec<String>,
    /// Installed or sampled machine facts supplied by the collector.
    pub live_facts: Vec<String>,
}
