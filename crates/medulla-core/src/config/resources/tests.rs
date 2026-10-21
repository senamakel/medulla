use super::*;

#[test]
fn resources_interval_defaults_and_clamps_during_deserialization() {
    let defaults = ResourcesConfig::default();
    assert_eq!(defaults.update_ms, 2_000);
    assert_eq!(defaults.shown_boxes, "cpu,mem,net,agents");
    assert!(defaults.presets.starts_with("cpu:0:default"));
    let low: ResourcesConfig = serde_json::from_str(r#"{"updateMs":1}"#).unwrap();
    let high: ResourcesConfig = serde_json::from_str(r#"{"updateMs":50000}"#).unwrap();
    assert_eq!(low.update_ms, MIN_RESOURCES_UPDATE_MS);
    assert_eq!(high.update_ms, MAX_RESOURCES_UPDATE_MS);
}

#[test]
fn resources_layout_settings_deserialize_as_camel_case() {
    let config: ResourcesConfig = serde_json::from_str(
        r#"{"shownBoxes":"cpu,agents","presets":"cpu:1:block","cpuBottom":true,"memBelowNet":true,"agentsLeft":true}"#,
    ).unwrap();
    assert_eq!(config.shown_boxes, "cpu,agents");
    assert_eq!(config.presets, "cpu:1:block");
    assert_eq!(config.cpu_bottom, Some(true));
    assert_eq!(config.mem_below_net, Some(true));
    assert_eq!(config.agents_left, Some(true));
}
