//! Joining a run's durable record to the harness sessions it has in flight.
//!
//! `workflow_run_get` reads the store, and the store only learns about a node
//! once that node has *finished*: a step is appended when the engine reports it
//! settled. So for the twenty minutes an `agent` node spends being a whole
//! coding session, the record says nothing about it at all — the caller sees a
//! run that is `running` with one fewer step than it expects and has no way to
//! tell a harness working from a harness wedged.
//!
//! The missing half is not in the store, and it is not in the engine either. It
//! is in the hub, which watched the dispatch go out and is watching the frames
//! come back. This module reaches for it.
//!
//! # The join
//!
//! Every `agent` dispatch is tagged `wf:{run_id}:{route}#{sequence}` (see
//! [`crate::flow_engine::caps::agent`]), and the hub's activity log records
//! which worker each outstanding task id is running on. That id is therefore
//! the whole of the correlation: filter the roster's `running` lists by the
//! `wf:{run_id}:` prefix and what is left is exactly this run's live harness
//! sessions, attributed to machines.
//!
//! # What this cannot see
//!
//! The harness *transcript* — what the session is typing, which files it has
//! touched, how far through it thinks it is. The hub carries a work snapshot on
//! each frame it observes, but no control-plane op exposes the activity log, so
//! the MCP server (a subprocess with no hub of its own) cannot read it. The
//! honest answer is therefore "this worker is running this step of your run",
//! not a progress bar, and the tool says so rather than implying more.
//!
//! Neither can it see a run executing in another process. The cancellation
//! registry is process-local, so `executingHere` is a fact about *this* server,
//! and a run started from the TUI reads as not executing here even while it is
//! plainly alive — which is why the fleet half is reported independently of it.

mod types;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use crate::control_socket::{ControlError, FleetWorker};
use crate::mcp::McpSession;
use crate::workflows::ops::{self, StepDetail};
use crate::workflows::WorkflowError;

use types::LiveDispatch;

/// The task-id prefix every `agent` dispatch of `run_id` carries.
///
/// Returned to the caller as well as used for filtering: a reader who wants to
/// grep a worker's own logs needs the same string, and deriving it from the
/// format string here is better than asking them to reconstruct it.
fn dispatch_prefix(run_id: &str) -> String {
    format!("wf:{run_id}:")
}

/// Split a task id into the route and sequence its suffix encodes.
///
/// `None` for anything that does not fit the shape. A task id is data from
/// another process, and a hub that one day tags a task differently should drop
/// out of this view rather than be reported with an invented route.
fn split_suffix(task_id: &str, prefix: &str) -> Option<(String, u64)> {
    let suffix = task_id.strip_prefix(prefix)?;
    let (route, sequence) = suffix.rsplit_once('#')?;
    Some((route.to_string(), sequence.parse().ok()?))
}

/// This run's live dispatches, drawn from the roster the control plane reports.
fn dispatches_in(workers: &[FleetWorker], prefix: &str) -> Vec<LiveDispatch> {
    let mut dispatches: Vec<LiveDispatch> = workers
        .iter()
        .flat_map(|worker| {
            worker.running.iter().filter_map(move |task_id| {
                let (route, sequence) = split_suffix(task_id, prefix)?;
                Some(LiveDispatch {
                    task_id: task_id.clone(),
                    route,
                    sequence,
                    worker: worker.id.clone(),
                    harness: worker.harness.clone(),
                    workspace: worker.workspace.clone(),
                })
            })
        })
        .collect();
    // Dispatch order, which is the order an operator reading this thinks in.
    // The roster's own order is the hub's map iteration and is not stable.
    dispatches.sort_by_key(|dispatch| dispatch.sequence);
    dispatches
}

/// Ask the control plane for the roster, or say why it could not be asked.
///
/// A fleet that is unreachable is not a failure of this tool: the durable half
/// of the answer is still worth having, and a caller told "no fleet" can act on
/// that. So every outcome that is not a roster comes back as prose to inline
/// beside the record rather than as an error that loses it.
async fn roster(session: &McpSession) -> Result<Vec<FleetWorker>, String> {
    if !session.families.fleet {
        return Err(
            "this session was not granted the fleet tools, so the live harness view is \
             withheld; the run record below is complete as far as the store knows"
                .to_string(),
        );
    }
    // `worker.list` probes each worker for its workflow catalogue on the way
    // past, which is wasted here — but it is the only op that reports what a
    // worker is running, and the probe is bounded server-side at two seconds.
    let answer = session
        .fleet
        .call("worker.list", json!({}))
        .await
        .map_err(describe)?;
    serde_json::from_value(answer["workers"].clone())
        .map_err(|err| format!("the fleet answered with a roster this build cannot read: {err}"))
}

/// Why the fleet could not be asked, phrased for the model that will read it.
fn describe(error: ControlError) -> String {
    match error {
        ControlError::NoInstance => {
            "no Medulla fleet is reachable from this session, so what its harnesses are doing \
             cannot be seen from here — the run record below is all there is"
                .to_string()
        }
        ControlError::Refused(failure) => failure.message,
        ControlError::Disconnected(reason) | ControlError::Transport(reason) => reason,
    }
}

/// What to say about a run with no live dispatches, which is several situations.
///
/// The distinction worth drawing is between "nothing is running because nothing
/// is left to run" and "nothing is running that this host can see", because the
/// second is the one where the absence proves nothing.
fn note(settled: bool, executing_here: bool, dispatches: usize) -> &'static str {
    if settled {
        "this run has finished; its steps below are the whole of what it did"
    } else if dispatches > 0 {
        "the harness sessions listed are running now; what each one is doing inside its \
         session is not visible from here, only that the worker is still working on it"
    } else if executing_here {
        "this run is executing in this process but has no harness session in flight — it is \
         between steps, or on a node kind that runs in-process (a code, transform, or \
         http_request node)"
    } else {
        "no harness session for this run is running anywhere this host can see, and the run \
         is not executing in this process either; it may be running in another process (the \
         TUI, or a daemon), or it may have been interrupted without recording an outcome"
    }
}

/// One run, joined to whatever the fleet is doing for it right now.
///
/// The durable half comes from [`ops::get_run`] at whatever step detail the
/// caller asked for, so this is a superset of `workflow_run_get` rather than a
/// competing shape — a caller can switch between the two without relearning the
/// record.
///
/// # Errors
///
/// Returns [`WorkflowError::NotFound`] for a run id the store has never seen.
/// A fleet that cannot be reached is *not* an error; it is reported inside the
/// answer, because the run record is still the thing that was asked for.
pub(crate) async fn detail(
    session: &McpSession,
    run_id: &str,
    steps: StepDetail,
) -> Result<Value, WorkflowError> {
    let record = ops::get_run(&session.store, run_id, steps)?;
    // Read off the projected record rather than tracked separately: the two
    // could otherwise disagree about a run that settled between the read and
    // the roster call, and the record is the half that was actually returned.
    let settled = record
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| !matches!(status, "running" | "pending_approval"));
    let executing_here = crate::workflows::run::is_running(run_id);
    let prefix = dispatch_prefix(run_id);

    // `executingHere` doubles as the answer to "can I stop this?" —
    // `workflow_run_cancel` reaches the same process-local registry — so it is
    // reported once under the name that says what it *is* rather than twice.
    let mut live = json!({
        "executingHere": executing_here,
        "taskIdPrefix": prefix,
    });
    let object = live.as_object_mut().expect("a json object");
    let dispatches = match roster(session).await {
        Ok(workers) => {
            let dispatches = dispatches_in(&workers, &prefix);
            object.insert("harnesses".to_string(), json!(dispatches));
            dispatches.len()
        }
        Err(reason) => {
            object.insert("fleetUnavailable".to_string(), json!(reason));
            0
        }
    };
    object.insert(
        "note".to_string(),
        json!(note(settled, executing_here, dispatches)),
    );

    Ok(json!({ "run": record, "live": live }))
}
