//! Data types for the joined view a run-detail call answers with.

use serde::Serialize;

/// One harness session the fleet is currently running on this run's behalf.
///
/// Assembled from two halves that only meet here: the task id, which the
/// engine minted as `wf:{run}:{route}#{sequence}` when it dispatched an `agent`
/// node, and the worker row the control plane reports that id as running on.
/// Neither half knows about the other — the engine never learns which machine
/// took the work, and the roster never learns which workflow step a task id
/// belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LiveDispatch {
    /// The dispatched task id, verbatim, so it can be quoted into a worker-side
    /// log search or handed to `fleet_abort`.
    pub(super) task_id: String,
    /// The `agent_ref` this node routed to, or `default` when it named none.
    ///
    /// The route rather than the node id: the id the engine encodes is the
    /// *worker address suffix* it resolved, which is what a reader correlating
    /// this against the graph's `agent_ref` fields is actually looking for.
    pub(super) route: String,
    /// Which dispatch of this run it is, counting from zero.
    ///
    /// Two concurrent `agent` nodes routed to the same worker differ only by
    /// this, so it is the only thing that tells them apart in a log.
    pub(super) sequence: u64,
    /// The roster id of the worker running it.
    pub(super) worker: String,
    /// The coding harness that worker runs — `claude`, `codex`, `opencode`.
    pub(super) harness: String,
    /// The directory it works in, when the worker declared one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) workspace: Option<String>,
}
