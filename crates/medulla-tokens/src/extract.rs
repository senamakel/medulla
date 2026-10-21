//! Provider-keyed token usage extraction and cumulative-counter folding.

use medulla_types::{HarnessProvider, TokenUsage};
use serde_json::Value;

/// Find token counts in a provider transcript or an already-isolated usage
/// object. Provider-specific total counters are preferred when the complete
/// record is supplied (notably Codex's cumulative `total_token_usage`).
pub fn scan_usage(provider: HarnessProvider, value: &Value) -> Option<TokenUsage> {
    if provider == HarnessProvider::Codex {
        if let Some(total) = value.pointer("/payload/info/total_token_usage") {
            return scan_usage_object(total, 0);
        }
    }
    scan_usage_object(value, 0)
}

fn scan_usage_object(value: &Value, depth: usize) -> Option<TokenUsage> {
    if depth > 4 {
        return None;
    }
    if let Some(obj) = value.as_object() {
        let num = |keys: [&str; 2]| {
            keys.iter().find_map(|k| obj.get(*k)).and_then(Value::as_i64)
        };
        if let (Some(input_tokens), Some(output_tokens)) = (
            num(["input_tokens", "inputTokens"]),
            num(["output_tokens", "outputTokens"]),
        ) {
            if input_tokens >= 0 && output_tokens >= 0 {
                return Some(TokenUsage { input_tokens, output_tokens });
            }
        }
        if let Some(tokens) = obj.get("tokens").and_then(Value::as_object) {
            let number = |key: &str| tokens.get(key).and_then(Value::as_i64);
            if let (Some(input_tokens), Some(output_tokens)) = (number("input"), number("output")) {
                if input_tokens >= 0 && output_tokens >= 0 {
                    return Some(TokenUsage { input_tokens, output_tokens });
                }
            }
        }
        return obj.values().find_map(|child| scan_usage_object(child, depth + 1));
    }
    None
}

/// How much of `current` is new since `previous`, given cumulative counters.
/// A decrease in either counter means the provider reset the pair.
pub fn usage_delta(previous: TokenUsage, current: TokenUsage) -> (i64, i64) {
    if current.input_tokens < previous.input_tokens || current.output_tokens < previous.output_tokens {
        (current.input_tokens.max(0), current.output_tokens.max(0))
    } else {
        (current.input_tokens - previous.input_tokens, current.output_tokens - previous.output_tokens)
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
