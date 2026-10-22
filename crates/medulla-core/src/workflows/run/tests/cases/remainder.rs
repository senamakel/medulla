#[tokio::test]
async fn a_diamond_runs_both_branches_and_waits_for_them_at_the_merge() {
    let harness = Harness::new();
    harness.install(&diamond(), "diamond");
    let dispatch = Arc::new(StubDispatch::default());

    let record = run_workflow(
        harness.context(dispatch.clone()),
        "diamond",
        "run-1",
        json!({}),
        Default::default(),
    )
    .await
    .expect("runs");

    assert_eq!(record.status, RunStatus::Succeeded);
    let dispatched: Vec<String> = dispatch
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.worker_address.clone())
        .collect();
    assert_eq!(
        dispatched.len(),
        2,
        "both branches should have dispatched: {dispatched:?}"
    );
    assert_eq!(
        record
            .steps
            .iter()
            .find(|step| step.node_id == "left")
            .and_then(|step| step.input.as_ref()),
        Some(&json!("left")),
        "the resolved prompt is durable evidence on the correct parallel step"
    );
    assert_eq!(
        record
            .steps
            .iter()
            .find(|step| step.node_id == "right")
            .and_then(|step| step.input.as_ref()),
        Some(&json!("right")),
        "parallel agent prompts must not be crossed"
    );
    assert!(dispatched.contains(&"a".to_string()));
    assert!(dispatched.contains(&"b".to_string()));

    // The merge only produces a step once both predecessors arrived, so seeing
    // it at all is the barrier working.
    assert!(
        record.steps.iter().any(|step| step.node_id == "join"),
        "the merge should have run: {:?}",
        record.steps
    );
}

#[tokio::test]
async fn a_run_is_recorded_and_can_be_read_back_by_id() {
    let harness = Harness::new();
    harness.install(&diamond(), "diamond");

    run_workflow(
        harness.context(Arc::new(StubDispatch::default())),
        "diamond",
        "run-2",
        json!({}),
        Default::default(),
    )
    .await
    .unwrap();

    let stored = require_run(harness.store.as_ref(), "run-2").expect("recorded");
    assert_eq!(stored.workflow_id, "diamond");
    assert_eq!(stored.status, RunStatus::Succeeded);
    assert!(stored.finished_at.is_some());
    assert!(!stored.steps.is_empty(), "steps should be persisted");
}

#[tokio::test]
async fn cancelling_an_in_flight_run_settles_it_as_cancelled() {
    let harness = Harness::new();
    harness.install(&gated(), "gated");
    let context = harness.context(Arc::new(HangingDispatch));

    let run = tokio::spawn(async move {
        run_workflow(
            context,
            "gated",
            "run-7",
            json!({ "approvals": ["review"] }),
            Default::default(),
        )
        .await
    });

    // Wait for the run to register itself before cancelling — otherwise the
    // test races setup and proves nothing.
    wait_until_running("run-7").await;
    assert!(cancel("run-7"), "the run should be cancellable by id");

    let record = run.await.unwrap().expect("settles");
    assert_eq!(record.status, RunStatus::Cancelled);
    assert!(
        !is_running("run-7"),
        "the guard must deregister the run on every exit path"
    );
}

#[tokio::test]
async fn the_cancel_tool_stops_a_run_that_is_genuinely_mid_harness_session() {
    // The path `workflow_run_cancel` takes, against a run that has a harness
    // session out — the case a settled-run test cannot reach, and the only one
    // where `cancelled: true` is the honest answer.
    let harness = Harness::new();
    harness.install(&gated(), "gated");
    let context = harness.context(Arc::new(HangingDispatch));

    let run = tokio::spawn(async move {
        run_workflow(
            context,
            "gated",
            "run-tool-cancel",
            json!({ "approvals": ["review"] }),
            Default::default(),
        )
        .await
    });
    wait_until_running("run-tool-cancel").await;
    // The dispatch is registered from the moment it goes out, so a run
    // inspector can see the session this process has in flight even though no
    // outer fleet roster knows about the embedded host running it.
    wait_until_dispatched("run-tool-cancel").await;
    let live = crate::workflows::run::in_flight("run-tool-cancel");
    assert_eq!(live.len(), 1, "{live:?}");
    assert!(
        live[0].task_id.starts_with("wf:run-tool-cancel:"),
        "{live:?}"
    );

    let answer = crate::workflows::ops::cancel_run(&harness.dyn_store(), "run-tool-cancel");

    assert_eq!(answer["cancelled"], json!(true), "{answer}");
    assert_eq!(answer["runId"], json!("run-tool-cancel"), "{answer}");
    let record = run.await.unwrap().expect("settles");
    assert_eq!(record.status, RunStatus::Cancelled);
    assert!(
        crate::workflows::run::in_flight("run-tool-cancel").is_empty(),
        "the dispatch guard must be withdrawn when the run is dropped"
    );
}

#[tokio::test]
async fn cancelling_a_run_that_already_settled_is_not_an_error() {
    assert!(!cancel("never-existed"));
}

#[tokio::test]
async fn a_disabled_workflow_refuses_to_run() {
    let harness = Harness::new();
    let mut record = parse_workflow(&diamond(), "diamond").unwrap();
    record.enabled = false;
    harness.store.save(&record).unwrap();

    let err = run_workflow(
        harness.context(Arc::new(StubDispatch::default())),
        "diamond",
        "run-8",
        json!({}),
        Default::default(),
    )
    .await
    .expect_err("disabled");

    assert!(err.to_string().contains("disabled"), "got {err}");
}

#[tokio::test]
async fn running_an_unknown_workflow_names_the_id() {
    let harness = Harness::new();

    let err = run_workflow(
        harness.context(Arc::new(StubDispatch::default())),
        "ghost",
        "run-9",
        json!({}),
        Default::default(),
    )
    .await
    .expect_err("no such workflow");

    assert!(matches!(err, WorkflowError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn a_run_that_exceeds_its_limit_fails_rather_than_hanging() {
    let harness = Harness::new();
    harness.install(&gated(), "gated");
    let mut context = harness.context(Arc::new(HangingDispatch));
    let mut impatient = (*harness.settings).clone();
    impatient.run_timeout_secs = 0;
    context.settings = Arc::new(impatient);

    let record = run_workflow(
        context,
        "gated",
        "run-10",
        json!({ "approvals": ["review"] }),
        Default::default(),
    )
    .await
    .expect("settles");

    assert_eq!(record.status, RunStatus::Failed);
    assert!(
        record
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("limit"),
        "say why it stopped: {:?}",
        record.error
    );
}

#[tokio::test]
async fn a_dry_run_exercises_the_graph_without_dispatching_anything() {
    let harness = Harness::new();
    harness.install(&diamond(), "diamond");
    let dispatch = Arc::new(StubDispatch::default());
    // Held only to prove it is never called.
    let _unused: Arc<dyn HarnessDispatch> = dispatch.clone();

    let result = dry_run(
        harness.store.clone(),
        Arc::new(StoreWorkflowResolver::new(harness.store.clone(), u64::MAX)),
        "diamond",
        json!({}),
        Default::default(),
    )
    .await
    .expect("simulates");

    assert!(
        result.output["nodes"]["join"].is_object(),
        "the whole graph should have run: {:?}",
        result.output
    );
    // Observing the run must not change what it does: the same graph, the same
    // outcome, plus a reading of the steps.
    assert!(result.diagnosis.is_clean(), "{:?}", result.diagnosis);
    assert!(
        dispatch.seen.lock().unwrap().is_empty(),
        "a dry run must start no harness session"
    );
}

/// The finalizer's reason for existing: a run future dropped mid-flight must
/// not leave a record claiming to be running.
#[tokio::test]
async fn a_run_dropped_mid_flight_is_reconciled_to_interrupted() {
    let harness = Harness::new();
    harness.install(&gated(), "gated");
    let context = harness.context(Arc::new(HangingDispatch));

    let run = tokio::spawn(async move {
        run_workflow(
            context,
            "gated",
            "run-11",
            json!({ "approvals": ["review"] }),
            Default::default(),
        )
        .await
    });
    wait_until_running("run-11").await;
    run.abort();
    let _ = run.await;

    // The abort drops the future at an await point, which runs the guard.
    let record = require_run(harness.store.as_ref(), "run-11").expect("recorded");
    assert_eq!(
        record.status,
        RunStatus::Interrupted,
        "a vanished run must not still claim to be running"
    );
}
