//! Rich usage values and numeric accumulators shared by SDK and TUI.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Token accounting reported alongside an inference or task result.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    /// Tokens consumed by the prompt.
    #[serde(rename = "inputTokens")]
    pub input_tokens: i64,
    /// Tokens produced by the model.
    #[serde(rename = "outputTokens")]
    pub output_tokens: i64,
    /// Prompt tokens served from the provider's cache, when reported.
    #[serde(
        default,
        rename = "cacheReadTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_read_tokens: Option<i64>,
    /// Prompt tokens written into the provider's cache, when reported.
    #[serde(
        default,
        rename = "cacheCreationTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_creation_tokens: Option<i64>,
}

impl Usage {
    /// Share of prompt tokens served from cache, when known.
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let cached = self.cache_read_tokens? as f64;
        let total = self.input_tokens as f64;
        (total > 0.0).then(|| (cached / total).clamp(0.0, 1.0))
    }
}

/// Token totals for one inference tier.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TierUsage {
    /// Prompt tokens billed to the tier.
    pub input_tokens: i64,
    /// Completion tokens billed to the tier.
    pub output_tokens: i64,
    /// Number of inference calls started on the tier.
    pub calls: i64,
}

/// Numeric totals folded from the event stream.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct UsageFold {
    /// Per-tier totals.
    pub tiers: BTreeMap<String, TierUsage>,
    /// Aggregate delegated-task usage.
    pub subagent: TierUsage,
    /// Per-task `(task id, input, output)` rows.
    pub tasks: Vec<(String, i64, i64)>,
}

impl UsageFold {
    /// Mutable access to a tier's totals, inserting a zeroed entry on first use.
    pub fn tier_mut(&mut self, tier: &str) -> &mut TierUsage {
        self.tiers.entry(tier.to_string()).or_default()
    }
}

/// A lane's accumulated token usage. Input is latest-wins; output accumulates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaneUsage {
    /// Prompt tokens on the most recent reported turn.
    pub input: i64,
    /// Output tokens summed across the lane.
    pub output: i64,
    /// Cache-read tokens on the most recent reported turn, when reported.
    pub cache_read: Option<i64>,
}

impl LaneUsage {
    /// Fold one reported usage into the running totals.
    pub fn accumulate(&mut self, usage: &Usage) {
        self.input = usage.input_tokens;
        self.output += usage.output_tokens;
        if usage.cache_read_tokens.is_some() {
            self.cache_read = usage.cache_read_tokens;
        }
    }

    /// Share of current prompt served from cache, when known.
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let cached = self.cache_read? as f64;
        (self.input > 0).then(|| (cached / self.input as f64).clamp(0.0, 1.0))
    }
}
