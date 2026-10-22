//! Tests for running, pausing, resuming, and cancelling a workflow.
//!
//! Every run here goes through the real engine with the real capability seam;
//! only the harness dispatch is a stand-in, because the alternative is starting
//! a coding agent.

mod cancellation;
mod continuation;
mod finalize;
mod loops;

use std::collections::HashMap;
pub(super) use std::sync::{Arc, Mutex};

use async_trait::async_trait;
pub(super) use serde_json::json;

pub(super) use super::super::{
    cancel, dry_run, is_running, resume_workflow, run_workflow, RunContext,
};
pub(super) use crate::flow_engine::caps::dispatch::HarnessDispatch;
use crate::flow_engine::{null_sink, CapabilitySettings, HostServices};
pub(super) use crate::hub::{RunError, TaskOutcome, TaskRequest};
use crate::workflows::store::parse_workflow;
pub(super) use crate::workflows::{
    require_run, FileWorkflowStore, RunStatus, StoreWorkflowResolver, WorkflowError, WorkflowStore,
};

/// A dispatch that answers immediately, recording what it saw.
#[derive(Default)]
pub(super) struct StubDispatch {
    pub(super) seen: Mutex<Vec<TaskRequest>>,
}

#[async_trait]
impl HarnessDispatch for StubDispatch {
    async fn dispatch(&self, request: TaskRequest) -> Result<TaskOutcome, RunError> {
        self.seen.lock().unwrap().push(request);
        Ok(TaskOutcome {
            reply: "done".into(),
            usage: crate::protocol::TokenUsage {
                input_tokens: 0,
                output_tokens: 0,

                cache_read: None,
                cache_write: None,
                reasoning: None,
            },
            harness: None,
            session_id: None,
            transcript: Vec::new(),
        })
    }
}

/// A dispatch that never returns, so a test can cancel a run that is genuinely
/// in flight rather than one that has already finished.
pub(super) struct HangingDispatch;

#[async_trait]
impl HarnessDispatch for HangingDispatch {
    async fn dispatch(&self, _request: TaskRequest) -> Result<TaskOutcome, RunError> {
        std::future::pending().await
    }
}

/// A resumed leg that swallows one node error before remaining in flight.
pub(super) struct ErrorThenHangDispatch;

#[async_trait]
impl HarnessDispatch for ErrorThenHangDispatch {
    async fn dispatch(&self, request: TaskRequest) -> Result<TaskOutcome, RunError> {
        if request.instruction == "fail" {
            return Err(RunError::Worker("expected failure".into()));
        }
        if request.instruction == "hang" {
            return std::future::pending().await;
        }
        Ok(TaskOutcome {
            reply: "approved".into(),
            usage: crate::protocol::TokenUsage {
                input_tokens: 0,
                output_tokens: 0,

                cache_read: None,
                cache_write: None,
                reasoning: None,
            },
            harness: None,
            session_id: None,
            transcript: Vec::new(),
        })
    }
}

/// A store, its settings, and a context factory over a temporary directory.
pub(super) struct Harness {
    _root: tempfile::TempDir,
    /// File-backed workflow and run state used by the test.
    pub(super) store: Arc<FileWorkflowStore>,
    /// Capability policy copied into each test run context.
    pub(super) settings: Arc<CapabilitySettings>,
}

impl Harness {
    /// Create an isolated permissive workflow harness.
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(FileWorkflowStore::new(
            vec![root.path().join("workflows")],
            root.path().join("runs"),
        ));
        let mut settings = CapabilitySettings::rooted_at(root.path());
        settings.default_worker_address = "worker".into();
        Self {
            _root: root,
            store,
            settings: Arc::new(settings),
        }
    }

    /// The same store behind the trait object the ops layer takes.
    pub(super) fn dyn_store(&self) -> Arc<dyn crate::workflows::WorkflowStore> {
        self.store.clone()
    }

    /// Parse and save one workflow fixture under `id`.
    pub(super) fn install(&self, document: &str, id: &str) {
        let record = parse_workflow(document, id).expect("valid fixture");
        self.store.save(&record).expect("saves");
    }

    /// Build a run context using the supplied stand-in dispatch.
    pub(super) fn context(&self, dispatch: Arc<dyn HarnessDispatch>) -> RunContext {
        RunContext {
            // Runs inline, so claiming at the top of the run is early enough.
            claim: None,
            store: self.store.clone(),
            settings: self.settings.clone(),
            services: HostServices {
                node_progress: None,
                dispatch,
                resolver: Arc::new(StoreWorkflowResolver::new(
                    self.store.clone(),
                    self.settings.max_loop_iterations,
                )),
                http_credentials: HashMap::new(),
            },
            sink: null_sink(),
            step_snapshot: None,
            origin: None,
        }
    }
}

/// Wait for a run to enter the process registry, failing instead of hanging.
async fn wait_until_running(run_id: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !is_running(run_id) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{run_id} was not registered"));
}

/// Wait until `run_id` has a harness dispatch recorded as in flight.
///
/// Registering the run and dispatching its first node are separate steps, so a
/// test that checked the dispatch registry the instant the run appeared would
/// race the engine rather than observe it.
async fn wait_until_dispatched(run_id: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while crate::workflows::run::in_flight(run_id).is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{run_id} never dispatched a harness session"));
}

/// A diamond: the trigger fans out to two agent nodes that run concurrently,
/// then a `merge` waits for both. Exercises parallel execution and the fan-in
/// barrier in one graph.
pub(super) fn diamond() -> String {
    json!({
        "id": "diamond",
        "name": "Diamond",
        "nodes": [
            { "id": "t", "kind": "trigger", "name": "start",
              "config": { "trigger_kind": "manual" } },
            { "id": "left", "kind": "agent", "name": "Left",
              "config": { "prompt": "left", "agent_ref": "a" } },
            { "id": "right", "kind": "agent", "name": "Right",
              "config": { "prompt": "right", "agent_ref": "b" } },
            { "id": "join", "kind": "merge", "name": "Join" }
        ],
        "edges": [
            { "from_node": "t", "to_node": "left" },
            { "from_node": "t", "to_node": "right" },
            { "from_node": "left", "to_node": "join" },
            { "from_node": "right", "to_node": "join" }
        ]
    })
    .to_string()
}

/// A graph whose single step is an approval gate.
pub(super) fn gated() -> String {
    json!({
        "id": "gated",
        "name": "Gated",
        "nodes": [
            { "id": "t", "kind": "trigger", "name": "start",
              "config": { "trigger_kind": "manual" } },
            { "id": "review", "kind": "agent", "name": "Review",
              "config": { "prompt": "review it", "requires_approval": true } }
        ],
        "edges": [{ "from_node": "t", "to_node": "review" }]
    })
    .to_string()
}

/// A gated graph whose resumed leg swallows an error and then hangs.
pub(super) fn gated_error_then_hang() -> String {
    json!({
        "id": "gated-error-then-hang",
        "name": "Gated error then hang",
        "nodes": [
            { "id": "t", "kind": "trigger", "name": "start",
              "config": { "trigger_kind": "manual" } },
            { "id": "review", "kind": "agent", "name": "Review",
              "config": { "prompt": "review it", "requires_approval": true } },
            { "id": "swallowed", "kind": "agent", "name": "Swallowed",
              "config": { "prompt": "fail", "on_error": "continue" } },
            { "id": "hang", "kind": "agent", "name": "Hang",
              "config": { "prompt": "hang" } }
        ],
        "edges": [
            { "from_node": "t", "to_node": "review" },
            { "from_node": "review", "to_node": "swallowed" },
            { "from_node": "swallowed", "to_node": "hang" }
        ]
    })
    .to_string()
}

include!("remainder.rs");
