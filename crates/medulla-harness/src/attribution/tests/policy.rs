//! Verifies attribution configuration and provider-specific arguments.

use super::{
    attribution_args, attribution_trailer, HarnessProvider, ATTRIBUTION_EMAIL, ATTRIBUTION_NAME,
};

#[test]
fn trailer_uses_the_medulla_identity() {
    assert_eq!(
        attribution_trailer(),
        "Co-authored-by: Medulla <medulla@tinyhumans.ai>"
    );
    assert_eq!(ATTRIBUTION_NAME, "Medulla");
    assert_eq!(ATTRIBUTION_EMAIL, "medulla@tinyhumans.ai");
}

#[test]
fn claude_receives_inline_settings_carrying_the_trailer() {
    let args = attribution_args(HarnessProvider::Claude, true);
    assert_eq!(args.len(), 2, "expected a flag/value pair, got {args:?}");
    assert_eq!(args[0], "--settings");

    let parsed: serde_json::Value =
        serde_json::from_str(&args[1]).expect("settings payload must be valid JSON");
    assert_eq!(
        parsed["attribution"]["commit"],
        serde_json::Value::String(attribution_trailer()),
    );
}

/// The payload must carry *only* `attribution.commit`, so it layers over the
/// operator's own settings without clobbering unrelated keys.
#[test]
fn claude_settings_payload_is_minimal() {
    let args = attribution_args(HarnessProvider::Claude, true);
    let parsed: serde_json::Value = serde_json::from_str(&args[1]).unwrap();

    let top = parsed.as_object().expect("payload is a JSON object");
    assert_eq!(top.len(), 1, "unexpected top-level keys: {top:?}");
    let attribution = parsed["attribution"]
        .as_object()
        .expect("attribution is a JSON object");
    assert_eq!(attribution.len(), 1, "unexpected keys: {attribution:?}");
}

/// Codex hardcodes its own trailer and Opencode has no knob at all, so neither
/// receives CLI args — they are attributed by the hook instead.
#[test]
fn providers_without_a_knob_receive_no_args() {
    for provider in [HarnessProvider::Codex, HarnessProvider::Opencode] {
        assert!(
            attribution_args(provider, true).is_empty(),
            "{provider:?} should receive no attribution args"
        );
    }
}

#[test]
fn disabling_suppresses_args_for_every_provider() {
    for provider in [
        HarnessProvider::Claude,
        HarnessProvider::Codex,
        HarnessProvider::Opencode,
    ] {
        assert!(
            attribution_args(provider, false).is_empty(),
            "{provider:?} should receive no args when off"
        );
    }
}
