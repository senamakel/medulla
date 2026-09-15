//! What is cached about a remote host's workflow store, and what each
//! outstanding request will fill in.

use std::collections::HashMap;

use medulla::workflows::{
    RunRecord, WorkflowNote, WorkflowProposal, WorkflowRecord, WorkflowRevision, WorkflowSummary,
};

/// Where the Workflows tab reads from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowSource {
    /// This device's layered store.
    Local,
    /// A remote host's store, through its cache.
    Remote {
        /// The `[[remoteHosts]]` id.
        host_id: String,
    },
}

/// What an outstanding `WorkflowCall` will fill in when it is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteWorkflowIntent {
    /// The catalogue.
    List,
    /// One workflow's record, by id.
    Get(String),
    /// One workflow's runs.
    Runs(String),
    /// One workflow's revisions.
    Revisions(String),
    /// One workflow's notes.
    Notes(String),
    /// One workflow's proposals.
    Proposals(String),
    /// A run was requested; the answer is its admission.
    Run(String),
    /// A workflow's deletion was requested.
    Delete(String),
}

/// Everything a remote host has answered about its store.
#[derive(Debug, Default)]
pub struct RemoteWorkflowCache {
    /// The catalogue, as last listed.
    pub summaries: Vec<WorkflowSummary>,
    /// Whole records, by workflow id.
    pub records: HashMap<String, WorkflowRecord>,
    /// Runs, by workflow id, newest first as the host lists them.
    pub runs: HashMap<String, Vec<RunRecord>>,
    /// Revisions, by workflow id.
    pub revisions: HashMap<String, Vec<WorkflowRevision>>,
    /// Notes, by workflow id.
    pub notes: HashMap<String, Vec<WorkflowNote>>,
    /// Proposals, by workflow id.
    pub proposals: HashMap<String, Vec<WorkflowProposal>>,
    /// The last thing that went wrong, shown on the tab.
    pub error: Option<String>,
    /// Whether the catalogue has been answered at least once.
    pub listed: bool,
    /// Requests in flight, by id.
    pub pending: HashMap<String, RemoteWorkflowIntent>,
    /// Counter for request ids.
    pub next_request: u64,
}

impl RemoteWorkflowCache {
    /// Apply one answer.
    ///
    /// # Errors
    ///
    /// When the answer does not decode as what the intent asked for — a host
    /// running a different version, most likely.
    pub fn fold(
        &mut self,
        intent: &RemoteWorkflowIntent,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let decode = |what: &str, error: serde_json::Error| format!("{what}: {error}");
        match intent {
            RemoteWorkflowIntent::List => {
                self.summaries = serde_json::from_value(value).map_err(|e| decode("catalogue", e))?;
                self.listed = true;
                // A workflow that vanished from the catalogue takes its cached
                // detail with it, so a stale record cannot be selected.
                let ids: Vec<String> = self.summaries.iter().map(|s| s.id.clone()).collect();
                self.records.retain(|id, _| ids.contains(id));
                self.runs.retain(|id, _| ids.contains(id));
            }
            RemoteWorkflowIntent::Get(id) => {
                let record: WorkflowRecord =
                    serde_json::from_value(value).map_err(|e| decode("workflow", e))?;
                self.records.insert(id.clone(), record);
            }
            RemoteWorkflowIntent::Runs(id) => {
                self.runs.insert(
                    id.clone(),
                    serde_json::from_value(value).map_err(|e| decode("runs", e))?,
                );
            }
            RemoteWorkflowIntent::Revisions(id) => {
                self.revisions.insert(
                    id.clone(),
                    serde_json::from_value(value).map_err(|e| decode("revisions", e))?,
                );
            }
            RemoteWorkflowIntent::Notes(id) => {
                self.notes.insert(
                    id.clone(),
                    serde_json::from_value(value).map_err(|e| decode("notes", e))?,
                );
            }
            RemoteWorkflowIntent::Proposals(id) => {
                self.proposals.insert(
                    id.clone(),
                    serde_json::from_value(value).map_err(|e| decode("proposals", e))?,
                );
            }
            // Status answers; nothing to keep.
            RemoteWorkflowIntent::Run(_)
            | RemoteWorkflowIntent::Cancel(_)
            | RemoteWorkflowIntent::Delete(_) => {}
        }
        Ok(())
    }
}
