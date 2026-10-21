//! Provider-related configuration types shared with the core SDK.

use std::collections::HashMap;

use medulla_types::BudgetWindow;
use serde::{Deserialize, Serialize};

/// Per-provider endpoint override for a router.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RouterProviderConfig {
    /// OpenAI-compatible (or Anthropic-passthrough) endpoint for this provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Optional custom inference router configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RouterConfig {
    /// Top-level endpoint for providers without an override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Environment variable name holding the API key, never the key itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Tier to model/SKU mapping.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub models: HashMap<String, String>,
    /// Per-provider overrides keyed by provider id.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub providers: HashMap<String, RouterProviderConfig>,
    /// OpenRouter serving provider slugs allowed to answer a request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_only: Vec<String>,
}

impl RouterConfig {
    /// Resolve the endpoint, preferring a non-empty provider override.
    pub fn base_url_for(&self, provider: &str) -> Option<&str> {
        self.providers
            .get(provider)
            .and_then(|p| p.base_url.as_deref())
            .filter(|s| !s.is_empty())
            .or(self.base_url.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// Return the configured model/SKU for a tier.
    pub fn model_for_tier(&self, tier: &str) -> Option<&str> {
        self.models.get(tier).map(String::as_str)
    }
}

/// Operator-declared budget numbers for one provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderBudgetConfig {
    /// Opaque seat/subscription label; never a credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    /// The metering window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<BudgetWindow>,
    /// Configured allowance for the window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_tokens: Option<i64>,
    /// Consumption recorded so far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_tokens: Option<i64>,
    /// Explicit remaining allowance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_tokens: Option<i64>,
    /// Unix seconds until which the seat is parked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<i64>,
}

/// Optional per-provider budget configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BudgetConfig {
    /// Configured provider budgets keyed by provider id.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub providers: HashMap<String, ProviderBudgetConfig>,
}

impl BudgetConfig {
    /// Return configured budget numbers for a provider id.
    pub fn for_provider(&self, provider: &str) -> Option<&ProviderBudgetConfig> {
        self.providers.get(provider)
    }
}
