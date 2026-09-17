//! A read-only [`WorkflowStore`] over a remote host's cache.
//!
//! Every read answers from what the host has sent so far; every write is
//! refused with the host's name in the reason, so the copilot and the editor
//! cannot silently author into a cache that nothing persists.

use std::sync::{Arc, Mutex};

use medulla::workflows::{
    RunRecord, WorkflowError, WorkflowNote, WorkflowProposal, WorkflowRecord, WorkflowRevision,
    WorkflowStore, WorkflowSummary,
};

use super::RemoteWorkflowCache;

/// The store the Workflows tab reads when scoped to a remote host.
pub struct RemoteWorkflowStore {
    host_id: String,
    cache: Arc<Mutex<RemoteWorkflowCache>>,
}

impl RemoteWorkflowStore {
    /// A store over `cache` for the host called `host_id`.
    pub fn new(host_id: &str, cache: Arc<Mutex<RemoteWorkflowCache>>) -> Self {
        RemoteWorkflowStore {
            host_id: host_id.to_string(),
            cache,
        }
    }

    fn read_only(&self) -> WorkflowError {
        WorkflowError::Engine(format!(
            "read-only: {} is a remote host; switch to this device with H to edit",
            self.host_id
        ))
    }

    fn with<T>(&self, read: impl FnOnce(&RemoteWorkflowCache) -> T) -> T {
        read(&self.cache.lock().expect("remote workflow cache"))
    }
}

impl WorkflowStore for RemoteWorkflowStore {
    fn proposal_decision_scope(&self) -> String {
        format!("remote:{}", self.host_id)
    }

    fn list(&self) -> Result<Vec<WorkflowSummary>, WorkflowError> {
        self.with(|cache| match &cache.error {
            Some(error) if !cache.listed => Err(WorkflowError::Engine(error.clone())),
            _ => Ok(cache.summaries.clone()),
        })
    }

    fn get(&self, id: &str) -> Result<Option<WorkflowRecord>, WorkflowError> {
        Ok(self.with(|cache| cache.records.get(id).cloned()))
    }

    fn save(&self, _record: &WorkflowRecord) -> Result<(), WorkflowError> {
        Err(self.read_only())
    }

    fn delete(&self, _id: &str) -> Result<(), WorkflowError> {
        Err(self.read_only())
    }

    fn record_run(&self, _run: &RunRecord) -> Result<(), WorkflowError> {
        Err(self.read_only())
    }

    fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, WorkflowError> {
        Ok(self.with(|cache| {
            cache
                .runs
                .values()
                .flatten()
                .find(|run| run.id == run_id)
                .cloned()
        }))
    }

    fn list_runs(&self, workflow_id: &str) -> Result<Vec<RunRecord>, WorkflowError> {
        Ok(self.with(|cache| cache.runs.get(workflow_id).cloned().unwrap_or_default()))
    }

    fn list_revisions(&self, workflow_id: &str) -> Result<Vec<WorkflowRevision>, WorkflowError> {
        Ok(self.with(|cache| {
            cache
                .revisions
                .get(workflow_id)
                .cloned()
                .unwrap_or_default()
        }))
    }

    fn revision(
        &self,
        workflow_id: &str,
        revision_id: &str,
    ) -> Result<Option<WorkflowRevision>, WorkflowError> {
        Ok(self.with(|cache| {
            cache
                .revisions
                .get(workflow_id)
                .and_then(|revisions| revisions.iter().find(|r| r.id == revision_id))
                .cloned()
        }))
    }

    fn list_notes(&self, workflow_id: &str) -> Result<Vec<WorkflowNote>, WorkflowError> {
        Ok(self.with(|cache| cache.notes.get(workflow_id).cloned().unwrap_or_default()))
    }

    fn append_note(&self, _note: &WorkflowNote) -> Result<(), WorkflowError> {
        Err(self.read_only())
    }

    fn save_proposal(&self, _proposal: &WorkflowProposal) -> Result<(), WorkflowError> {
        Err(self.read_only())
    }

    fn get_proposal(&self, id: &str) -> Result<Option<WorkflowProposal>, WorkflowError> {
        Ok(self.with(|cache| {
            cache
                .proposals
                .values()
                .flatten()
                .find(|proposal| proposal.id == id)
                .cloned()
        }))
    }

    fn list_proposals(&self, workflow_id: &str) -> Result<Vec<WorkflowProposal>, WorkflowError> {
        Ok(self.with(|cache| {
            cache
                .proposals
                .get(workflow_id)
                .cloned()
                .unwrap_or_default()
        }))
    }
}
