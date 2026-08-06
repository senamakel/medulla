//! The in-flight harness dispatch registry.
//!
//! A run started in this process dispatches its `agent` nodes onto a
//! [`LocalWorkflowHost`](crate::workflows::local) — an embedded daemon the run
//! owns and drops with itself. That host is nobody else's business: it does not
//! register with the outer control plane, so the fleet roster a
//! `worker.list` returns has no row for it and no `wf:{run}:…` task id in any
//! `running` list. Asking the fleet what a locally-started run is doing
//! therefore answers "nothing", however busy the run is.
//!
//! The dispatcher that *does* know is the capability itself. This is where it
//! says so: [`record`] is held across the dispatch await and dropped when the
//! harness replies, so [`in_flight`] is exactly the set of sessions this
//! process has out at this instant.
//!
//! Process-local, like [`super::registry`] and for the same reason — a run
//! executing in the TUI is invisible here, which is why a reader joins this
//! with the fleet roster rather than choosing between them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::workflows::RunId;

/// One harness session this process has dispatched and not yet had back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlightDispatch {
    /// The dispatched task id, `wf:{run}:{route}#{sequence}`.
    pub task_id: String,
    /// The worker address it went to.
    pub worker: String,
    /// The harness it resolved to, or empty when the node left the choice to
    /// the worker's own configured harness.
    pub harness: String,
    /// The directory the run works in, when the host set one.
    pub workspace: Option<String>,
}

/// Dispatches outstanding per run, in this process.
type Registry = Mutex<HashMap<RunId, Vec<InFlightDispatch>>>;

/// The process-wide registry.
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A recorded dispatch, withdrawn when dropped.
///
/// An RAII guard rather than a matched pair of calls because the dispatch await
/// can be cancelled — a run being dropped mid-node is the ordinary shape of
/// `workflow_run_cancel` — and a leaked entry would have the tool reporting a
/// harness session that ended minutes ago.
pub struct DispatchGuard {
    run_id: RunId,
    task_id: String,
}

/// Record `dispatch` as in flight for `run_id` until the guard drops.
#[must_use]
pub fn record(run_id: &str, dispatch: InFlightDispatch) -> DispatchGuard {
    let guard = DispatchGuard {
        run_id: run_id.to_string(),
        task_id: dispatch.task_id.clone(),
    };
    registry()
        .lock()
        .expect("dispatch registry lock")
        .entry(run_id.to_string())
        .or_default()
        .push(dispatch);
    guard
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        let mut runs = match registry().lock() {
            Ok(runs) => runs,
            // Another dispatch panicked while holding the lock. Nothing safe to
            // do here; the stale entry is the lesser harm.
            Err(_) => return,
        };
        let Some(dispatches) = runs.get_mut(&self.run_id) else {
            return;
        };
        // Task ids are unique per run — the sequence is a per-run counter — so
        // removing the first match removes this guard's own entry.
        if let Some(at) = dispatches
            .iter()
            .position(|dispatch| dispatch.task_id == self.task_id)
        {
            dispatches.remove(at);
        }
        // Keep the map from growing one empty vector per run the process ever
        // ran.
        if dispatches.is_empty() {
            runs.remove(&self.run_id);
        }
    }
}

/// The harness sessions `run_id` has out right now, in this process.
///
/// Empty for a run executing elsewhere, which proves nothing about it — see the
/// module docs.
pub fn in_flight(run_id: &str) -> Vec<InFlightDispatch> {
    registry()
        .lock()
        .expect("dispatch registry lock")
        .get(run_id)
        .cloned()
        .unwrap_or_default()
}

/// A shared handle for a capability that wants to record its own dispatches.
///
/// The `agent` capability is constructed with the run id it belongs to and
/// nothing else, so this is just that id in a form the capability can hold.
#[derive(Debug, Clone)]
pub struct DispatchRecorder {
    run_id: Arc<str>,
    workspace: Option<String>,
}

impl DispatchRecorder {
    /// A recorder for `run_id`, tagging dispatches with `workspace`.
    pub fn new(run_id: &str, workspace: Option<String>) -> Self {
        Self {
            run_id: Arc::from(run_id),
            workspace,
        }
    }

    /// Record one dispatch until the returned guard drops.
    #[must_use]
    pub fn record(&self, task_id: &str, worker: &str, harness: &str) -> DispatchGuard {
        record(
            &self.run_id,
            InFlightDispatch {
                task_id: task_id.to_string(),
                worker: worker.to_string(),
                harness: harness.to_string(),
                workspace: self.workspace.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch(task_id: &str) -> InFlightDispatch {
        InFlightDispatch {
            task_id: task_id.to_string(),
            worker: "local".to_string(),
            harness: "claude".to_string(),
            workspace: Some("/tmp/work".to_string()),
        }
    }

    #[test]
    fn a_recorded_dispatch_is_visible_until_its_guard_drops() {
        let run = "run-visible";
        assert!(in_flight(run).is_empty());
        {
            let _guard = record(run, dispatch("wf:run-visible:default#0"));
            let live = in_flight(run);
            assert_eq!(live.len(), 1);
            assert_eq!(live[0].task_id, "wf:run-visible:default#0");
            assert_eq!(live[0].harness, "claude");
        }
        assert!(in_flight(run).is_empty());
    }

    #[test]
    fn concurrent_dispatches_withdraw_independently() {
        let run = "run-concurrent";
        let first = record(run, dispatch("wf:run-concurrent:default#0"));
        let _second = record(run, dispatch("wf:run-concurrent:reviewer#1"));
        assert_eq!(in_flight(run).len(), 2);
        drop(first);
        let live = in_flight(run);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].task_id, "wf:run-concurrent:reviewer#1");
    }

    #[test]
    fn a_run_with_nothing_in_flight_reports_nothing() {
        assert!(in_flight("run-never-dispatched").is_empty());
    }

    #[test]
    fn a_recorder_tags_dispatches_with_its_run_and_workspace() {
        let recorder = DispatchRecorder::new("run-recorder", Some("/srv/app".to_string()));
        let _guard = recorder.record("wf:run-recorder:default#0", "local", "codex");
        let live = in_flight("run-recorder");
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].worker, "local");
        assert_eq!(live[0].harness, "codex");
        assert_eq!(live[0].workspace.as_deref(), Some("/srv/app"));
    }
}
