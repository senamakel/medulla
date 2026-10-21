//! Regression tests for provider resolution and routing primitives.

use std::collections::HashMap;

use medulla_types::HarnessProvider;

use crate::{
    bin_is_overridden, can_resume, model_override, provider_bin, router_env, RouterConfig,
};

#[test]
fn provider_binary_and_model_overrides_keep_the_existing_precedence() {
    let env = HashMap::from([
        ("MEDULLA_CLAUDE_BIN".into(), "  /opt/claude  ".into()),
        ("MEDULLA_CLAUDE_MODEL".into(), "  opus  ".into()),
        ("MEDULLA_HARNESS_MODEL".into(), "fallback".into()),
    ]);
    assert_eq!(provider_bin(HarnessProvider::Claude, &env), "/opt/claude");
    assert!(bin_is_overridden(HarnessProvider::Claude, "/opt/claude"));
    assert_eq!(model_override(HarnessProvider::Claude, &env).as_deref(), Some("opus"));
}

#[test]
fn router_environment_only_contains_endpoint_and_secret_name() {
    let router = RouterConfig {
        base_url: Some("https://router.example/v1".into()),
        api_key_env: Some("ROUTER_KEY".into()),
        ..RouterConfig::default()
    };
    let injected = router_env(HarnessProvider::Codex, &router);
    assert_eq!(injected.env, vec![("OPENAI_BASE_URL".into(), "https://router.example/v1".into())]);
    assert_eq!(injected.secret_env, vec![("OPENAI_API_KEY".into(), "ROUTER_KEY".into())]);
    assert!(router_env(HarnessProvider::Openhuman, &router).is_empty());
}

#[test]
fn resume_routing_matches_provider_capabilities() {
    assert!(can_resume(HarnessProvider::Claude));
    assert!(can_resume(HarnessProvider::Codex));
    assert!(!can_resume(HarnessProvider::Opencode));
}
