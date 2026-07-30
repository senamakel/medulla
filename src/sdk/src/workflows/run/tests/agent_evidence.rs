//! Durable prompt evidence captured from real agent-node execution.

use super::*;

#[tokio::test]
async fn an_agent_step_records_the_prompt_after_expression_resolution() {
    let harness = Harness::new();
    harness.install(
        &json!({
            "id": "resolved-prompt",
            "name": "Resolved prompt",
            "nodes": [
                { "id": "t", "kind": "trigger", "name": "start",
                  "config": { "trigger_kind": "manual" } },
                { "id": "work", "kind": "agent", "name": "Work",
                  "config": { "prompt": "=item.task", "agent_ref": "builder" } }
            ],
            "edges": [{ "from_node": "t", "to_node": "work" }]
        })
        .to_string(),
        "resolved-prompt",
    );
    let dispatch = Arc::new(StubDispatch::default());

    let record = run_workflow(
        harness.context(dispatch.clone()),
        "resolved-prompt",
        "run-resolved",
        json!({ "task": "Review the complete patch\nand report every risk." }),
    )
    .await
    .expect("runs");

    assert_eq!(
        dispatch.seen.lock().unwrap()[0].instruction,
        "Review the complete patch\nand report every risk."
    );
    assert_eq!(
        record.steps[0].input,
        Some(json!("Review the complete patch\nand report every risk."))
    );
}
