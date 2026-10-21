use super::*;

#[test]
fn resources_interval_defaults_and_clamps_during_deserialization() {
    assert_eq!(ResourcesConfig::default().update_ms, 2_000);
    let low: ResourcesConfig = serde_json::from_str(r#"{"updateMs":1}"#).unwrap();
    let high: ResourcesConfig = serde_json::from_str(r#"{"updateMs":50000}"#).unwrap();
    assert_eq!(low.update_ms, MIN_RESOURCES_UPDATE_MS);
    assert_eq!(high.update_ms, MAX_RESOURCES_UPDATE_MS);
}
