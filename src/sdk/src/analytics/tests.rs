//! Unit tests for OpenPanel payload and opt-out behavior.

use super::{
    clear_profile, current_profile, identify_payload, is_truthy, set_profile, track_payload,
};

#[test]
fn identify_uses_the_authenticated_account_as_the_profile_id() {
    assert_eq!(
        identify_payload("user-42"),
        serde_json::json!({ "type": "identify", "payload": { "profileId": "user-42" } })
    );
}

#[test]
fn lifecycle_events_contain_no_user_content() {
    let payload = track_payload("application_started", "user-42", serde_json::json!({}));
    assert_eq!(payload["payload"]["profileId"], "user-42");
    assert_eq!(payload["payload"]["name"], "application_started");
    assert_eq!(payload["payload"]["properties"]["app"], "medulla");
    assert!(payload["payload"].get("prompt").is_none());
    assert!(payload["payload"].get("token").is_none());
}

#[test]
fn analytics_opt_out_accepts_common_truthy_values() {
    for value in ["1", "TRUE", " yes "] {
        assert!(is_truthy(value), "{value}");
    }
    assert!(!is_truthy("off"));
}

#[test]
fn clearing_the_profile_removes_the_authenticated_identity() {
    set_profile("user-42");
    clear_profile();
    assert_eq!(current_profile(), None);
}
