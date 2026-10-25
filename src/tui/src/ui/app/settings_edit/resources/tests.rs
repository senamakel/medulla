use super::*;

#[test]
fn every_resources_config_field_has_an_editor_row() {
    let config = serde_json::to_value(ResourcesConfig::default()).unwrap();
    let fields = config.as_object().unwrap();
    for field in fields.keys() {
        let camel = field.clone();
        assert!(
            ROWS.iter().any(|row| row.key == camel),
            "no Resources setting for {field}"
        );
    }
    assert_eq!(ROWS.len(), fields.len());
}

#[test]
fn optional_flags_cycle_between_auto_on_and_off_in_both_directions() {
    let auto = None;
    let on = Some(true);
    let off = Some(false);

    assert_eq!(optional_flag(auto, 1), on);
    assert_eq!(optional_flag(on, 1), off);
    assert_eq!(optional_flag(off, 1), auto);
    assert_eq!(optional_flag(auto, -1), off);
    assert_eq!(optional_flag(off, -1), on);
    assert_eq!(optional_flag(on, -1), auto);
}
