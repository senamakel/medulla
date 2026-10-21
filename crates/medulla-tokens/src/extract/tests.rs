use medulla_types::{HarnessProvider, TokenUsage};
use serde_json::{json, Value};

use super::{reported_usage_delta, scan_usage, usage_delta};

#[test]
fn scan_usage_finds_provider_shapes_in_nested_transcripts() {
    let cases: [(HarnessProvider, Value, TokenUsage); 4] = [
        (HarnessProvider::Claude, json!({"type":"result","usage":{"input_tokens":10,"output_tokens":2}}), TokenUsage { input_tokens: 10, output_tokens: 2 }),
        (HarnessProvider::Codex, json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":7,"output_tokens":3}}}}), TokenUsage { input_tokens: 7, output_tokens: 3 }),
        (HarnessProvider::Claude, json!({"usage":{"inputTokens":1,"outputTokens":9}}), TokenUsage { input_tokens: 1, output_tokens: 9 }),
        (HarnessProvider::Opencode, json!({"type":"part","part":{"tokens":{"input":12,"output":4,"reasoning":0}}}), TokenUsage { input_tokens: 12, output_tokens: 4 }),
    ];
    for (provider, value, expected) in cases {
        assert_eq!(scan_usage(provider, &value), Some(expected));
    }
}

#[test]
fn codex_total_is_preferred_to_its_per_call_sibling() {
    let record = json!({"payload":{"info":{
        "total_token_usage":{"input_tokens":900,"output_tokens":90},
        "last_token_usage":{"input_tokens":10,"output_tokens":2}
    }}});
    assert_eq!(scan_usage(HarnessProvider::Codex, &record), Some(TokenUsage { input_tokens: 900, output_tokens: 90 }));
}

#[test]
fn scan_skips_invalid_matches_and_continues_the_bounded_walk() {
    let value = json!({"input_tokens":-1,"output_tokens":2,"nested":{"input_tokens":3,"output_tokens":4}});
    assert_eq!(scan_usage(HarnessProvider::Claude, &value), Some(TokenUsage { input_tokens: 3, output_tokens: 4 }));
    assert_eq!(scan_usage(HarnessProvider::Claude, &json!({"usage":{"input_tokens":-0.5,"output_tokens":1}})), None);
}

#[test]
fn cumulative_usage_delta_handles_resets_as_one_pair() {
    let usage = |input_tokens, output_tokens| TokenUsage { input_tokens, output_tokens };
    assert_eq!(usage_delta(usage(10, 2), usage(50, 5)), (40, 3));
    assert_eq!(usage_delta(usage(10, 2), usage(10, 2)), (0, 0));
    assert_eq!(usage_delta(usage(1000, 10), usage(50, 20)), (50, 20));
    assert_eq!(reported_usage_delta(HarnessProvider::Codex, Some(usage(7, 3)), usage(0, 0), usage(900, 90)), (7, 3));
}
