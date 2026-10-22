use serde::{Deserialize, Serialize};
/// Token usage a responder reports for a completed task (child harness
/// consumption, surfaced to the orchestrator).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens the child harness consumed as input/prompt.
    #[serde(rename = "inputTokens")]
    pub input_tokens: i64,
    /// Tokens the child harness produced as output/completion.
    #[serde(rename = "outputTokens")]
    pub output_tokens: i64,
    /// Cached prompt tokens, when reported.
    #[serde(
        default,
        rename = "cacheReadTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_read: Option<i64>,
    /// Prompt tokens written to cache, when reported.
    #[serde(
        default,
        rename = "cacheWriteTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_write: Option<i64>,
    /// Reasoning tokens included in output, when reported.
    #[serde(
        default,
        rename = "reasoningTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning: Option<i64>,
}

/// The metering window a paid harness seat renews its allowance on.
///
/// Wire values are snake_case (`five_hour`), matching the provider enum. Defaults
/// to [`BudgetWindow::Unknown`] so a probe that cannot determine the window still
/// serializes a well-formed descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BudgetWindow {
    /// A per-day allowance.
    Daily,
    /// A per-week allowance.
    Weekly,
    /// The rolling five-hour window some subscriptions meter on.
    FiveHour,
    /// The metering window is not known.
    #[default]
    Unknown,
}
