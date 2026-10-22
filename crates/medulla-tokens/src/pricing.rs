//! Versioned, embedded provider pricing and token-cost estimates.

use std::sync::OnceLock;

use medulla_types::HarnessProvider;
use serde::Deserialize;

use crate::usage::Usage;

const PRICING_TOML: &str = include_str!("../pricing.toml");

/// Estimated token cost in US dollars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cost {
    /// Estimated USD amount, computed from the embedded pricing snapshot and
    /// its default cache-write TTL assumption.
    pub usd: f64,
}

#[derive(Debug, Deserialize)]
struct PricingTable {
    version: u32,
    model: Vec<ModelPrice>,
}

#[derive(Debug, Deserialize)]
struct ModelPrice {
    provider: String,
    model: String,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: Option<f64>,
    cache_write_default_ttl: Option<String>,
    input_includes_cache: bool,
}

fn table() -> &'static PricingTable {
    static TABLE: OnceLock<PricingTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let table: PricingTable =
            toml::from_str(PRICING_TOML).expect("embedded pricing.toml must parse");
        assert_eq!(table.version, 1, "unsupported embedded pricing version");
        table
    })
}

/// Estimate token cost from the bundled price snapshot. Cache writes use a
/// table-listed TTL because [`Usage`] does not carry one; if no write price is
/// listed for a model and usage includes cache writes, the estimate is unknown.
/// Unknown provider/model pairs return `None`; no model name matching or price
/// guessing is performed. Reasoning tokens are a subset of output tokens and
/// are not added a second time.
pub fn cost(provider: HarnessProvider, model: &str, usage: &Usage) -> Option<Cost> {
    let provider = match provider {
        HarnessProvider::Claude => "claude",
        HarnessProvider::Codex => "codex",
        HarnessProvider::Opencode => return None,
        HarnessProvider::Openhuman | HarnessProvider::Shell => return None,
    };
    let price = table()
        .model
        .iter()
        .find(|entry| entry.provider == provider && entry.model == model)?;
    debug_assert_eq!(
        price.cache_write.is_some(),
        price.cache_write_default_ttl.is_some()
    );
    let read = usage.cache_read_tokens.unwrap_or(0).max(0);
    let write = usage
        .cache_write_tokens
        .or(usage.cache_creation_tokens)
        .unwrap_or(0)
        .max(0);
    let cache_write_rate = if write > 0 { price.cache_write? } else { 0.0 };
    let uncached = if price.input_includes_cache {
        usage
            .input_tokens
            .saturating_sub(read)
            .saturating_sub(write)
            .max(0)
    } else {
        usage.input_tokens.max(0)
    };
    let usd = (uncached as f64 * price.input
        + read as f64 * price.cache_read
        + write as f64 * cache_write_rate
        + usage.output_tokens.max(0) as f64 * price.output)
        / 1_000_000.0;
    Some(Cost { usd })
}

#[cfg(test)]
mod tests {
    use medulla_types::HarnessProvider;

    use crate::usage::Usage;

    use super::cost;

    #[test]
    fn estimates_claude_cost_by_uncached_read_write_and_output_tokens() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 200_000,
            cache_read_tokens: Some(100_000),
            cache_creation_tokens: None,
            cache_write_tokens: Some(100_000),
            reasoning_tokens: Some(50_000),
        };
        assert_eq!(
            cost(HarnessProvider::Claude, "claude-sonnet-4-6", &usage)
                .unwrap()
                .usd,
            6.405
        );
    }

    #[test]
    fn estimates_codex_cost_and_returns_none_for_unknown_models() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: Some(100_000),
            cache_creation_tokens: None,
            cache_write_tokens: None,
            reasoning_tokens: Some(50_000),
        };
        assert_eq!(
            cost(HarnessProvider::Codex, "gpt-5.3-codex", &usage)
                .unwrap()
                .usd,
            2.9925
        );
        let without_reasoning = Usage {
            reasoning_tokens: None,
            ..usage.clone()
        };
        assert_eq!(
            cost(HarnessProvider::Codex, "gpt-5.3-codex", &usage),
            cost(HarnessProvider::Codex, "gpt-5.3-codex", &without_reasoning)
        );
        let with_unpriced_write = Usage {
            cache_write_tokens: Some(100_000),
            ..usage.clone()
        };
        assert_eq!(
            cost(
                HarnessProvider::Codex,
                "gpt-5.3-codex",
                &with_unpriced_write
            ),
            None
        );
        assert_eq!(cost(HarnessProvider::Codex, "unknown-model", &usage), None);
    }
}
