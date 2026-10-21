#[test]
fn sdk_config_reexport_reads_the_subscription_fixture_unchanged() {
    let source = include_str!("../../../../medulla-tokens/tests/fixtures/subscriptions.toml");
    let document: toml::Value = toml::from_str(source).expect("fixture TOML");
    let config: crate::config::SubscriptionsConfig = document["subscriptions"]
        .clone()
        .try_into()
        .expect("SDK config re-export parses the section");
    assert_eq!(config.claude_session, crate::config::ResourceDisplay::Bar);
    assert_eq!(config.claude_weekly, crate::config::ResourceDisplay::Percent);
    assert_eq!(config.claude_scoped, crate::config::ResourceDisplay::Off);
    assert_eq!(config.claude_credits, crate::config::ResourceDisplay::Value);
    assert_eq!(config.codex_session, crate::config::ResourceDisplay::Bar);
    assert_eq!(config.codex_weekly, crate::config::ResourceDisplay::Off);
    assert_eq!(config.open_router_credits, crate::config::ResourceDisplay::Percent);
    assert_eq!(config.tinyhumans_balance, crate::config::ResourceDisplay::Value);
    assert_eq!(config.refresh_seconds, 420);
    assert_eq!(config.open_router_key_env.as_deref(), Some("OPENROUTER_TEST_KEY"));
    let serialized = toml::Value::try_from(config).expect("section serializes");
    assert_eq!(serialized, document["subscriptions"]);
}
