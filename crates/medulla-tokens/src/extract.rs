//! Provider-keyed token usage extraction and cumulative-counter folding.

use medulla_types::{HarnessProvider, TokenUsage};
use serde_json::Value;

use crate::usage::Usage;

/// Find token counts in a provider transcript or an already-isolated usage
/// object. Provider-specific total counters are preferred when the complete
/// record is supplied (notably Codex's cumulative `total_token_usage`).
pub fn scan_usage(provider: HarnessProvider, value: &Value) -> Option<TokenUsage> {
    scan_rich_usage(provider, value).map(Into::into)
}

fn scan_rich_usage(provider: HarnessProvider, value: &Value) -> Option<Usage> {
    match provider {
        HarnessProvider::Claude => {
            if let Some(usage) = value
                .pointer("/message/usage")
                .or_else(|| value.pointer("/usage"))
                .and_then(|usage| scan_usage_object(usage, 0))
            {
                return Some(usage);
            }
        }
        HarnessProvider::Codex => {
            // The total is cumulative and must win over its per-call sibling.
            // A present but invalid total is deliberately not replaced by the
            // last-call value on the same record.
            if let Some(total) = value.pointer("/payload/info/total_token_usage") {
                return scan_usage_object(total, 0);
            }
        }
        HarnessProvider::Opencode => {
            if let Some(usage) = value
                .pointer("/part/tokens")
                .or_else(|| value.pointer("/tokens"))
                .and_then(|usage| scan_usage_object(usage, 0))
            {
                return Some(usage);
            }
        }
        HarnessProvider::Openhuman | HarnessProvider::Shell => {}
    }
    scan_usage_object(value, 0)
}

fn scan_usage_object(value: &Value, depth: usize) -> Option<Usage> {
    if depth > 4 {
        return None;
    }
    if let Some(obj) = value.as_object() {
        let num = |keys: [&str; 2]| {
            keys.iter()
                .find_map(|k| obj.get(*k))
                .and_then(Value::as_i64)
        };
        let optional = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| obj.get(*key).and_then(Value::as_i64))
        };
        let nested_cache = |key: &str| {
            obj.get("cache")
                .and_then(Value::as_object)
                .and_then(|cache| cache.get(key))
                .and_then(Value::as_i64)
        };
        let nested_detail = |group: &str, key: &str| {
            obj.get(group)
                .and_then(Value::as_object)
                .and_then(|detail| detail.get(key))
                .and_then(Value::as_i64)
        };
        let categories = || {
            (
                optional(&[
                    "cache_read_input_tokens",
                    "cached_input_tokens",
                    "cachedInputTokens",
                    "cacheReadTokens",
                    "cacheReadInputTokens",
                ])
                .or_else(|| nested_detail("input_tokens_details", "cached_tokens"))
                .or_else(|| nested_cache("read")),
                optional(&["cache_creation_input_tokens"]),
                optional(&[
                    "cache_write_input_tokens",
                    "cacheWriteInputTokens",
                    "cacheWriteTokens",
                ])
                .or_else(|| nested_detail("input_tokens_details", "cache_write_tokens"))
                .or_else(|| nested_cache("write")),
                optional(&[
                    "reasoning_output_tokens",
                    "reasoningOutputTokens",
                    "reasoningTokens",
                    "reasoning",
                ])
                .or_else(|| nested_detail("output_tokens_details", "reasoning_tokens")),
            )
        };
        if let (Some(input_tokens), Some(output_tokens)) = (
            num(["input_tokens", "inputTokens"]),
            num(["output_tokens", "outputTokens"]),
        ) {
            if input_tokens >= 0 && output_tokens >= 0 {
                let (
                    cache_read_tokens,
                    cache_creation_tokens,
                    cache_write_tokens,
                    reasoning_tokens,
                ) = categories();
                return Some(Usage {
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_creation_tokens,
                    cache_write_tokens,
                    reasoning_tokens,
                });
            }
        }
        if let Some(tokens) = obj.get("tokens").and_then(Value::as_object) {
            let number = |key: &str| tokens.get(key).and_then(Value::as_i64);
            let cache = tokens.get("cache").and_then(Value::as_object);
            if let (Some(input_tokens), Some(output_tokens)) = (number("input"), number("output")) {
                if input_tokens >= 0 && output_tokens >= 0 {
                    let (
                        category_cache_read,
                        cache_creation_tokens,
                        category_cache_write,
                        reasoning_tokens,
                    ) = categories();
                    return Some(Usage {
                        input_tokens,
                        output_tokens,
                        cache_read_tokens: number("cache_read")
                            .or_else(|| cache.and_then(|cache| cache.get("read"))?.as_i64())
                            .or(category_cache_read),
                        cache_creation_tokens,
                        cache_write_tokens: number("cache_write")
                            .or_else(|| cache.and_then(|cache| cache.get("write"))?.as_i64())
                            .or(category_cache_write),
                        reasoning_tokens: number("reasoning").or(reasoning_tokens),
                    });
                }
            }
        }
        return obj
            .values()
            .find_map(|child| scan_usage_object(child, depth + 1));
    }
    None
}

/// How much of `current` is new since `previous`, given cumulative counters.
/// A decrease in either counter means the provider reset the pair.
pub fn usage_delta(previous: TokenUsage, current: TokenUsage) -> (i64, i64) {
    if current.input_tokens < previous.input_tokens
        || current.output_tokens < previous.output_tokens
    {
        (current.input_tokens.max(0), current.output_tokens.max(0))
    } else {
        (
            current.input_tokens - previous.input_tokens,
            current.output_tokens - previous.output_tokens,
        )
    }
}

/// Usage reported for one provider's cumulative snapshot.
pub fn reported_usage_delta(
    provider: HarnessProvider,
    first_call: Option<TokenUsage>,
    previous: TokenUsage,
    total: TokenUsage,
) -> (i64, i64) {
    match (provider, first_call) {
        (HarnessProvider::Codex, Some(call)) => (call.input_tokens, call.output_tokens),
        _ => usage_delta(previous, total),
    }
}

#[cfg(test)]
mod tests;
