#[tokio::test]
async fn the_same_workflow_id_in_another_store_can_evolve_concurrently() {
    let id = "the-same-workflow-id-in-another-store-can-evolve";
    let (_first_home, first_store, _first_run) = fixture(id);
    let (_second_home, second_store, _second_run) = fixture(id);
    let first_scope = first_store.proposal_decision_scope();
    let _guard = EvolveGuard::claim(&first_scope, id).expect("first store claims its workflow");

    let outcome = session(second_store.clone(), silent(&second_store, "ok"))
        .evolve(id, EvolveTrigger::Manual, None)
        .await
        .expect("the other store can review its own workflow");

    assert!(!outcome.skipped);
}

#[tokio::test]
async fn the_claim_is_released_when_the_pass_ends() {
    let id = "the-claim-is-released-when-the-pass-ends";
    let (_home, store, _run) = fixture(id);

    session(store.clone(), silent(&store, "ok"))
        .evolve(id, EvolveTrigger::Manual, None)
        .await
        .expect("the pass completes");

    // The release is a `Drop` on the guard, so the assertion only means
    // anything if a claim was actually taken — hence the `expect` above rather
    // than swallowing an error, and hence running against the workflow the
    // fixture installed rather than one that does not exist.
    assert!(
        !is_evolving(&store.proposal_decision_scope(), id),
        "a finished pass releases its claim"
    );

    // And a second pass can now run, which is the thing the release is for.
    let again = session(store.clone(), silent(&store, "ok"))
        .evolve(id, EvolveTrigger::Manual, None)
        .await
        .expect("the pass completes");
    assert!(!again.skipped);
}

#[tokio::test]
async fn a_review_of_an_unknown_workflow_fails_rather_than_inventing_one() {
    let (_home, store, _run) = fixture("a-review-of-an-unknown-workflow-fails-ra");

    let failed = session(store.clone(), silent(&store, "ok"))
        .evolve("no-such-workflow", EvolveTrigger::Manual, None)
        .await;

    assert!(failed.is_err());
    assert!(
        !is_evolving(&store.proposal_decision_scope(), "no-such-workflow"),
        "and claims nothing"
    );
}

#[tokio::test]
async fn a_disabled_host_refuses_the_pass() {
    let (_home, store, _run) = fixture("a-disabled-host-refuses-the-pass");
    let mut evolving = session(store.clone(), silent(&store, "ok"));
    evolving.config.enabled = false;

    let failed = evolving
        .evolve(
            "a-disabled-host-refuses-the-pass",
            EvolveTrigger::Manual,
            None,
        )
        .await;

    assert!(failed.is_err());
    // Nothing is written either: a switched-off feature does not half-run.
    assert!(store
        .list_notes("a-disabled-host-refuses-the-pass")
        .expect("readable")
        .is_empty());
}

#[tokio::test]
async fn a_trigger_naming_a_lost_run_still_reviews_the_rest() {
    let (_home, store, _run) = fixture("a-trigger-naming-a-lost-run-still-review");

    let outcome = session(store.clone(), silent(&store, "ok"))
        .evolve(
            "a-trigger-naming-a-lost-run-still-review",
            EvolveTrigger::Failure("run-gone".into()),
            None,
        )
        .await
        .expect("a missing run does not sink the review");

    // No system note — there was no run record to write one from — but the
    // journal and the rest of the history are still there to work with.
    assert!(outcome.notes.is_empty());
    assert_eq!(outcome.reply, "ok");
}

#[tokio::test]
async fn a_trigger_refuses_a_successful_run() {
    let (_home, store, mut run) = fixture("a-trigger-refuses-a-successful-run");
    run.status = RunStatus::Succeeded;
    run.error = None;
    store.record_run(&run).expect("updates the run");

    let err = session(store.clone(), silent(&store, "must not dispatch"))
        .evolve(
            "a-trigger-refuses-a-successful-run",
            EvolveTrigger::Failure(run.id),
            None,
        )
        .await
        .expect_err("only failures can trigger failure review");

    assert!(err.to_string().contains("not a failed run"), "got: {err}");
}

#[tokio::test]
async fn a_trigger_refuses_a_run_from_another_workflow() {
    let (_home, store, mut run) = fixture("a-trigger-refuses-a-foreign-run");
    run.workflow_id = "some-other-workflow".into();
    store.record_run(&run).expect("updates the run");

    let err = session(store.clone(), silent(&store, "must not dispatch"))
        .evolve(
            "a-trigger-refuses-a-foreign-run",
            EvolveTrigger::Failure(run.id),
            None,
        )
        .await
        .expect_err("a workflow cannot review another workflow's run");

    assert!(
        err.to_string().contains("belongs to workflow"),
        "got: {err}"
    );
}

#[tokio::test]
async fn verification_allows_findings_already_present_in_the_base_graph() {
    let (_home, store, _run) = fixture("verification-compares-with-the-base");
    ops::create(
        &store,
        &json!({
            "id": "verification-compares-with-the-base",
            "nodes": [
                { "id": "t", "kind": "trigger", "name": "start",
                  "config": { "trigger_kind": "manual" } },
                { "id": "say", "kind": "tool_call", "name": "Say",
                  "config": { "slug": "medulla:echo", "on_error": "continue",
                              "args": { "text": "=run.trigger.absent" } } }
            ],
            "edges": [{ "from_node": "t", "to_node": "say" }]
        })
        .to_string(),
        "verification-compares-with-the-base",
    )
    .expect("installs input-dependent workflow");

    let proposed = ops::propose(
        &store,
        "verification-compares-with-the-base",
        "Clarify the step name without changing its bindings",
        &json!([{ "op": "set_node_name", "id": "say", "name": "Notify" }]),
        Vec::new(),
        Vec::new(),
    )
    .await
    .expect("proposal verifies");

    assert_eq!(proposed["ok"], json!(true), "{proposed}");
}

#[test]
fn the_config_is_switched_off_by_the_outer_workflows_switch() {
    let mut config = crate::config::WorkflowsConfig {
        enabled: false,
        ..Default::default()
    };
    assert!(
        !EvolveConfig::from_config(&config).enabled,
        "a host with workflows off has no runs to review"
    );

    config.enabled = true;
    config.evolve.enabled = false;
    assert!(!EvolveConfig::from_config(&config).enabled);

    config.evolve.enabled = true;
    assert!(EvolveConfig::from_config(&config).enabled);
}

#[tokio::test]
async fn a_review_turn_asks_for_the_restricted_tool_set() {
    let (_home, store, run) = fixture("a-review-turn-asks-for-restricted-tools");
    let stub = Arc::new(StubReviewer {
        reply: "ok".into(),
        edit: None,
        store: store.clone(),
        seen: Mutex::new(Vec::new()),
    });

    session(store.clone(), stub.clone())
        .evolve(
            "a-review-turn-asks-for-restricted-tools",
            EvolveTrigger::Failure(run.id),
            None,
        )
        .await
        .expect("the pass completes");

    // The whole autonomy argument rests on this reaching the harness. Asserted
    // on the dispatch rather than on `ToolMode` in isolation, because the mode
    // being *correct* and the mode being *sent* are different claims and only
    // the second one protects the graph.
    let seen = stub.seen.lock().unwrap();
    assert_eq!(
        seen[0].tool_mode.as_deref(),
        Some("propose:a-review-turn-asks-for-restricted-tools")
    );
}

#[test]
fn an_ordinary_copilot_turn_asks_for_nothing_and_gets_the_full_surface() {
    use crate::mcp::ToolMode;

    // The default has to stay permissive: every dispatch but a review's leaves
    // this unset, and a mistake here would silently disarm the copilot.
    assert_eq!(ToolMode::from_wire(None), ToolMode::Full);
    assert_eq!(ToolMode::from_wire(Some("propose")), ToolMode::Propose);
    assert_eq!(ToolMode::from_wire(Some("full")), ToolMode::Full);
    assert_eq!(ToolMode::from_wire(Some("execute")), ToolMode::Propose);
    // Round-trips, so a frame written by one build is read the same by another.
    assert_eq!(
        ToolMode::from_wire(Some(ToolMode::Propose.as_wire())),
        ToolMode::Propose
    );
}
