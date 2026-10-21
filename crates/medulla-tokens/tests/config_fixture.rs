use medulla_tokens::SubscriptionsConfig;

#[test]
fn subscriptions_fixture_round_trips_the_published_toml_shape() {
    let fixture = include_str!("fixtures/subscriptions.toml");
    let parsed: toml::Value = toml::from_str(fixture).expect("TOML fixture parses");
    let config: SubscriptionsConfig = parsed
        .get("subscriptions")
        .expect("subscriptions section")
        .clone()
        .try_into()
        .expect("subscriptions config parses");
    assert_eq!(config.claude_session, medulla_tokens::ResourceDisplay::Bar);
    assert_eq!(config.refresh_seconds, 420);
    assert_eq!(config.open_router_key_env.as_deref(), Some("OPENROUTER_TEST_KEY"));
    let serialized = toml::Value::try_from(config).expect("config serializes");
    assert_eq!(serialized, parsed["subscriptions"]);
}
