//! Tests for how an `agent` dispatch is addressed.
//!
//! The subject here is the task id, which is the whole of the correlation
//! between a run and the harness sessions it has out: the run inspector joins
//! on it, `fleet_abort` cancels by it, and a worker dedupes on it. A duplicate
//! is therefore not a cosmetic clash — it silently merges two sessions.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use async_trait::async_trait;

use super::{dispatch_harness, AgentRoute, HarnessAgentRunner};
use crate::flow_engine::caps::dispatch::HarnessDispatch;
use crate::flow_engine::harness_choice::HarnessChoice;
use crate::flow_engine::settings::CapabilitySettings;
use crate::hub::{RunError, TaskOutcome, TaskRequest};

/// A dispatch that is never actually reached: these tests stop at the request.
struct UnusedDispatch;

#[async_trait]
impl HarnessDispatch for UnusedDispatch {
    async fn dispatch(&self, _request: TaskRequest) -> Result<TaskOutcome, RunError> {
        unreachable!("these tests build requests rather than dispatching them")
    }
}

/// A runner for `run`, sharing `sequence` when one is given.
fn runner(run: &str, sequence: Option<Arc<AtomicU64>>) -> HarnessAgentRunner {
    let root = std::env::temp_dir().join("medulla-agent-tests");
    let mut settings = CapabilitySettings::rooted_at(&root);
    settings.default_worker_address = "worker".to_string();
    let built = HarnessAgentRunner::new(Arc::new(UnusedDispatch), Arc::new(settings), run);
    match sequence {
        Some(sequence) => built.with_sequence(sequence),
        None => built,
    }
}

/// The route both runners take when a node names no `agent_ref`.
fn task_id_of(runner: &HarnessAgentRunner) -> String {
    runner
        .request(
            &AgentRoute::Default,
            "do the thing".to_string(),
            HarnessChoice::default(),
        )
        .task_id
}

#[test]
fn one_runner_numbers_its_dispatches_in_order() {
    let runner = runner("run-ordered", None);
    assert_eq!(task_id_of(&runner), "wf:run-ordered:default#0");
    assert_eq!(task_id_of(&runner), "wf:run-ordered:default#1");
}

#[test]
fn two_runners_sharing_a_sequence_never_mint_the_same_task_id() {
    // The shape the run actually builds: an agent runner and the LLM provider's
    // own runner, both tagged with one run id and both routing to `default`.
    let sequence = Arc::new(AtomicU64::new(0));
    let agent = runner("run-shared", Some(sequence.clone()));
    let llm = runner("run-shared", Some(sequence));

    let first = task_id_of(&agent);
    let second = task_id_of(&llm);

    assert_ne!(
        first, second,
        "a shared sequence must not hand the same id to both runners"
    );
    assert_eq!(first, "wf:run-shared:default#0");
    assert_eq!(second, "wf:run-shared:default#1");
}

#[test]
fn independent_sequences_are_what_the_sharing_prevents() {
    // Guards the premise rather than the fix: if two unshared runners ever stop
    // colliding, `with_sequence` is no longer load-bearing and this test says so
    // instead of the collision resurfacing somewhere subtler.
    let agent = runner("run-split", None);
    let llm = runner("run-split", None);
    assert_eq!(task_id_of(&agent), task_id_of(&llm));
}
