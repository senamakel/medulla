//! Focused tests for folding ACP stream updates into semantic harness events.

use std::sync::{Arc, Mutex};

use crate::daemon::status_detail;

use super::FoldState;

#[test]
fn agent_message_chunks_form_one_reply() {
    let mut state = FoldState::new(None);
    for text in ["hello ", "world"] {
        let update = serde_json::from_value(serde_json::json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": text}
        }))
        .unwrap();
        state.fold(update);
    }
    assert_eq!(state.reply(), "hello world");
}

#[test]
fn non_text_updates_do_not_pollute_the_reply() {
    let mut state = FoldState::new(None);
    let update = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "call-1",
        "title": "Run tests",
        "kind": "execute",
        "status": "pending"
    }))
    .unwrap();
    state.fold(update);
    assert_eq!(
        state.reply(),
        "ACP agent completed without a text response."
    );
}

#[test]
fn tool_updates_preserve_failure_state_for_the_copilot() {
    let details = Arc::new(Mutex::new(Vec::new()));
    let captured = details.clone();
    let mut state = FoldState::new(Some(Box::new(move |event| {
        if let Some(detail) = status_detail(&event.event) {
            captured.lock().unwrap().push(detail);
        }
    })));
    let call = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "call-1",
        "title": "Terminal",
        "kind": "execute",
        "status": "in_progress",
        "rawInput": { "command": "cargo test --workspace" }
    }))
    .unwrap();
    let failure = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "call-1",
        "status": "failed",
        "rawOutput": "tests failed"
    }))
    .unwrap();
    let still_running = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "call-1",
        "status": "in_progress"
    }))
    .unwrap();

    state.fold(call);
    state.fold(still_running);
    state.fold(failure);

    assert_eq!(
        *details.lock().unwrap(),
        [
            "running Terminal · $ cargo test --workspace\u{1f}call-1",
            "tool failed\u{1f}call-1"
        ]
    );
}

#[test]
fn running_tool_patch_surfaces_the_command_when_it_arrives_late() {
    let details = Arc::new(Mutex::new(Vec::new()));
    let captured = details.clone();
    let mut state = FoldState::new(Some(Box::new(move |event| {
        if let Some(detail) = status_detail(&event.event) {
            captured.lock().unwrap().push(detail);
        }
    })));
    let call = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "call-1",
        "title": "Terminal",
        "kind": "execute",
        "status": "in_progress"
    }))
    .unwrap();
    let input = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "call-1",
        "status": "in_progress",
        "rawInput": { "command": "cargo test --workspace" }
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&input).unwrap()["rawInput"]["command"],
        "cargo test --workspace"
    );

    state.fold(call);
    state.fold(input);

    assert_eq!(
        *details.lock().unwrap(),
        [
            "running Terminal\u{1f}call-1",
            "running Terminal · $ cargo test --workspace\u{1f}call-1"
        ]
    );
}

#[test]
fn running_tool_patch_preserves_initial_metadata() {
    let details = Arc::new(Mutex::new(Vec::new()));
    let captured = details.clone();
    let mut state = FoldState::new(Some(Box::new(move |event| {
        if let Some(detail) = status_detail(&event.event) {
            captured.lock().unwrap().push(detail);
        }
    })));
    let call = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "call-1",
        "title": "Read configuration",
        "kind": "read",
        "status": "in_progress"
    }))
    .unwrap();
    let input = serde_json::from_value(serde_json::json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "call-1",
        "status": "in_progress",
        "rawInput": { "path": "/tmp/medulla.json" }
    }))
    .unwrap();

    state.fold(call);
    state.fold(input);

    assert_eq!(
        *details.lock().unwrap(),
        [
            "running Read: Read configuration\u{1f}call-1",
            "running Read · /tmp/medulla.json\u{1f}call-1"
        ]
    );
}

#[test]
fn thought_chunks_emit_a_cumulative_bounded_snapshot() {
    let thoughts = Arc::new(Mutex::new(Vec::new()));
    let captured = thoughts.clone();
    let mut state = FoldState::new(Some(Box::new(move |event| {
        if event.event.kind == "agent_thought" {
            captured
                .lock()
                .unwrap()
                .push(event.event.payload["text"].as_str().unwrap().to_string());
        }
    })));
    for text in ["Checking ", "the workflow.", &"x".repeat(1_000)] {
        let update = serde_json::from_value(serde_json::json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": text }
        }))
        .unwrap();
        state.fold(update);
    }

    let thoughts = thoughts.lock().unwrap();
    assert_eq!(thoughts[0], "Checking ");
    assert_eq!(thoughts[1], "Checking the workflow.");
    assert_eq!(thoughts[2].chars().count(), 780);
    assert!(thoughts[2].starts_with('…'));
}
