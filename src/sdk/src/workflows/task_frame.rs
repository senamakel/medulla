//! Running a saved workflow from a `medulla-task/1` task frame.
//!
//! A task frame naming a `workflow` runs that saved graph instead of handing its
//! text to a harness, with the *task id as the run id* — which is what makes
//! the sender's ordinary abort cancel the run, since every node the run
//! dispatches carries that id as its `abort_id`. That contract is
//! `docs/workflows.md`'s, written for the host link peers.

use serde_json::Value;

use crate::workflows::{run_workflow, RunContext, RunRecord, WorkflowError};

/// Run the saved workflow `workflow_id` for a task frame, under the task's id.
///
/// That identity is the whole contract: the run id is the engine checkpointer's
/// thread id *and* the `abort_id` on every task the run dispatches, so the
/// sender aborting the task it sent cancels the graph and the harness session
/// it is waiting on, with no workflow-specific abort path.
///
/// # Errors
///
/// Fails when workflows are disabled on this host, when the workflow is unknown
/// or disabled, when the graph does not compile, or when a run with this id is
/// already executing — a resent frame must not start the graph twice, because
/// both copies would run every node's side effects and race to overwrite the
/// same run record.
pub async fn run_task_workflow(
    context: RunContext,
    workflow_id: &str,
    task_id: &str,
    instruction: &str,
) -> Result<RunRecord, WorkflowError> {
    // No declared inputs: a task frame carries one instruction, not a named
    // argument set. A workflow that declares a required input therefore
    // refuses a task frame, with an error naming the input — which is the
    // honest answer until the frame protocol carries them.
    run_workflow(
        context,
        workflow_id,
        task_id,
        trigger_input(instruction),
        serde_json::Map::new(),
    )
    .await
}

/// Cancel the run started for `task_id`, reporting whether one was cancelled.
///
/// **Process-local**, and honestly so: the run registry lives in this process,
/// so this cancels a run *this* host started and nothing else. That is not a
/// limitation on the path that matters — an abort frame arrives at the host
/// executing the run, which is the one holding the registry — but a caller
/// relaying an abort on someone else's behalf will get `false` and must not read
/// it as "already finished".
pub fn cancel_task_workflow(task_id: &str) -> bool {
    crate::workflows::run::cancel(task_id)
}

/// The trigger payload for a run, from a task frame's text.
///
/// JSON when the sender sent JSON, and `{ "text": … }` otherwise — so a frame
/// carrying an ordinary instruction still gives the graph's expressions
/// something to read. Empty text becomes an empty object rather than
/// `{"text":""}`, which would look like a deliberate blank input.
pub fn trigger_input(text: &str) -> Value {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(trimmed).unwrap_or_else(|_| serde_json::json!({ "text": text }))
}

#[cfg(test)]
#[path = "task_frame_tests.rs"]
mod tests;
