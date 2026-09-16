//! Tests for answering `WorkflowCall`s: store reads come back as the store's
//! own records, and everything else goes through the MCP dispatcher.

use std::sync::Arc;

use serde_json::json;

use super::serve_workflow_call;
use crate::mcp::{McpSession, ToolMode};
use crate::workflows::{FileWorkflowStore, WorkflowStore};

fn document(id: &str) -> String {
    json!({
        "id": id,
        "name": "Sweep",
        "description": "sweeps",
        "nodes": [
            { "id": "t", "kind": "trigger", "name": "start",
              "config": { "trigger_kind": "manual" } },
            { "id": "work", "kind": "agent", "name": "Work",
              "config": { "prompt": "do it", "agent_ref": "builder" } }
        ],
        "edges": [{ "from_node": "t", "to_node": "work" }]
    })
    .to_string()
}

fn fixture() -> (tempfile::TempDir, Arc<dyn WorkflowStore>, McpSession) {
    let root = tempfile::tempdir().unwrap();
    let store: Arc<dyn WorkflowStore> = Arc::new(FileWorkflowStore::new(
        vec![root.path().join("workflows")],
        root.path().join("runs"),
    ));
    crate::workflows::ops::create(&store, &document("sweep"), "sweep").expect("creates");
    let session = McpSession::local(store.clone(), Default::default(), ToolMode::Full);
    (root, store, session)
}

#[tokio::test]
async fn store_reads_answer_with_the_records_themselves() {
    let (_root, store, session) = fixture();
    let listed = serve_workflow_call(&store, &session, "store_list", json!({}))
        .await
        .unwrap();
    assert_eq!(listed[0]["id"], "sweep");

    let record = serve_workflow_call(&store, &session, "store_get", json!({ "id": "sweep" }))
        .await
        .unwrap();
    // The whole record, graph included — not the MCP projection.
    assert_eq!(record["graph"]["nodes"].as_array().unwrap().len(), 2);

    let runs = serve_workflow_call(
        &store,
        &session,
        "store_list_runs",
        json!({ "id": "sweep" }),
    )
    .await
    .unwrap();
    assert_eq!(runs, json!([]));
    for tool in [
        "store_list_revisions",
        "store_list_notes",
        "store_list_proposals",
    ] {
        let value = serve_workflow_call(&store, &session, tool, json!({ "id": "sweep" }))
            .await
            .unwrap();
        assert!(value.is_array(), "{tool}: {value}");
    }
}

#[tokio::test]
async fn a_missing_record_or_argument_is_a_named_error() {
    let (_root, store, session) = fixture();
    let missing = serve_workflow_call(&store, &session, "store_get", json!({ "id": "nope" }))
        .await
        .unwrap_err();
    assert!(missing.contains("nope"), "{missing}");
    let no_arg = serve_workflow_call(&store, &session, "store_get_run", json!({}))
        .await
        .unwrap_err();
    assert!(no_arg.contains("runId"), "{no_arg}");
}

#[tokio::test]
async fn other_names_are_forwarded_to_the_mcp_dispatcher() {
    let (_root, store, session) = fixture();
    let listed = serve_workflow_call(&store, &session, "workflow_list", json!({}))
        .await
        .unwrap();
    assert_eq!(listed["workflows"][0]["id"], "sweep");

    let unknown = serve_workflow_call(&store, &session, "workflow_bogus", json!({}))
        .await
        .unwrap_err();
    assert!(unknown.contains("unknown tool"), "{unknown}");

    // A tool that fails reports its own error, unwrapped from the envelope.
    let failed = serve_workflow_call(&store, &session, "workflow_get", json!({ "id": "nope" }))
        .await
        .unwrap_err();
    assert!(!failed.is_empty());
}
