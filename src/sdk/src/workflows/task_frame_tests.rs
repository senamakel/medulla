//! Tests for the task-frame → workflow-run seam.

use serde_json::json;

use super::*;

#[test]
fn trigger_input_passes_json_through_and_wraps_anything_else() {
    assert_eq!(
        trigger_input(r#"{"repo":"medulla"}"#),
        json!({"repo":"medulla"})
    );
    assert_eq!(trigger_input("ship it"), json!({ "text": "ship it" }));
    // Blank is an empty payload, not a deliberate blank string.
    assert_eq!(trigger_input("   "), json!({}));
}

#[test]
fn cancelling_a_task_nobody_is_running_reports_false_rather_than_pretending() {
    // Process-local by construction: this host cannot cancel another's run, and
    // says so instead of returning a success the caller would misread.
    assert!(!cancel_task_workflow("task-that-was-never-started"));
}
