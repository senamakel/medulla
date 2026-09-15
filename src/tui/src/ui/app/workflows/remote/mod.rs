//! The Workflows tab on a remote host.
//!
//! When `H` points the TUI at another machine, the Workflows tab shows *that*
//! machine's catalogue, runs and history. Nothing on the tab reads a store
//! directly — every reader goes through [`App::workflow_store`] — so the whole
//! of "remote" is a second [`WorkflowStore`] implementation over a cache, and
//! the tab's render and key paths are untouched.
//!
//! The cache is filled by answers to `WorkflowCall`s: the tab issues a read
//! when it needs something the cache lacks, the host answers off the serve
//! loop, and the answer is folded in by
//! [`App::fold_remote_workflow_result`]. Writes never reach the host through
//! the store — authoring stays on this device — but running, cancelling and
//! deleting go over as forwarded MCP tools, where the host's own policy admits
//! them.
//!
//! [`App::workflow_store`]: crate::ui::app::App::workflow_store

mod store;
mod types;

#[cfg(test)]
mod tests;

pub use types::{RemoteWorkflowCache, RemoteWorkflowIntent, WorkflowSource};

use std::sync::{Arc, Mutex};

use medulla::workflows::WorkflowStore;

use super::super::hosts::HostSelection;
use super::super::types::{App, Cmd};
use crate::remote::client::RemoteRequest;
pub(in crate::ui::app) use store::RemoteWorkflowStore;

impl App {
    /// Where the Workflows tab reads from under the current host selection.
    ///
    /// `All` reads this device: a union of every host's store would have no
    /// single place to write to and no single set of runs to number, so the
    /// combined view keeps the local catalogue and says so in its title.
    pub(in crate::ui::app) fn workflow_source(&self) -> WorkflowSource {
        match &self.current_host {
            HostSelection::Host(id) => WorkflowSource::Remote {
                host_id: id.clone(),
            },
            HostSelection::ThisDevice | HostSelection::All => WorkflowSource::Local,
        }
    }

    /// The cache for `host_id`, created empty on first use.
    pub(in crate::ui::app) fn remote_workflow_cache(
        &mut self,
        host_id: &str,
    ) -> Arc<Mutex<RemoteWorkflowCache>> {
        self.remote_workflows
            .entry(host_id.to_string())
            .or_default()
            .clone()
    }

    /// A store over the cache for `host_id`.
    pub(in crate::ui::app) fn remote_workflow_store(&self, host_id: &str) -> Arc<dyn WorkflowStore> {
        let cache = self
            .remote_workflows
            .get(host_id)
            .cloned()
            .unwrap_or_default();
        Arc::new(RemoteWorkflowStore::new(host_id, cache))
    }

    /// Ask the current remote host for something, remembering what the answer
    /// is for.
    ///
    /// Does nothing when the host has not been dialled: the dial is what the
    /// selection itself triggers, and the tab re-asks once it answers.
    pub(in crate::ui::app) fn remote_workflow_request(
        &mut self,
        host_id: &str,
        intent: RemoteWorkflowIntent,
        tool: &str,
        args: serde_json::Value,
    ) {
        let cache = self.remote_workflow_cache(host_id);
        let request_id = {
            let mut cache = cache.lock().expect("remote workflow cache");
            // One outstanding request per intent: a rail that asks for the
            // same run list on every tick must not queue a hundred answers.
            if cache.pending.values().any(|pending| *pending == intent) {
                return;
            }
            cache.next_request += 1;
            let request_id = format!("wf-{}-{}", host_id, cache.next_request);
            cache.pending.insert(request_id.clone(), intent);
            request_id
        };
        let Some(state) = self.remote_hosts.get_mut(host_id) else {
            return;
        };
        if let Err(reason) = state.request(RemoteRequest::WorkflowCall {
            request_id: request_id.clone(),
            tool: tool.to_string(),
            args,
        }) {
            let mut cache = cache.lock().expect("remote workflow cache");
            cache.pending.remove(&request_id);
            cache.error = Some(reason);
        }
    }

    /// Ask for the catalogue, and for whatever the selection needs beside it.
    pub(in crate::ui::app) fn refresh_remote_workflows(&mut self, host_id: &str) {
        self.remote_workflow_request(
            host_id,
            RemoteWorkflowIntent::List,
            "store_list",
            serde_json::json!({}),
        );
    }

    /// Fetch what the selected workflow needs that the cache lacks.
    ///
    /// Called from the same places the local tab re-reads its files, so a
    /// remote workflow is fetched when it is selected and its runs re-read on
    /// the same cadence. Every request here is idempotent and deduplicated, so
    /// calling it on every reload is cheap.
    pub(in crate::ui::app) fn ensure_remote_workflow_loaded(&mut self, host_id: &str, id: &str) {
        let (has_record, has_runs) = {
            let cache = self.remote_workflow_cache(host_id);
            let cache = cache.lock().expect("remote workflow cache");
            (
                cache.records.contains_key(id),
                cache.runs.contains_key(id),
            )
        };
        let by_id = serde_json::json!({ "id": id });
        if !has_record {
            self.remote_workflow_request(
                host_id,
                RemoteWorkflowIntent::Get(id.to_string()),
                "store_get",
                by_id.clone(),
            );
        }
        if !has_runs {
            self.remote_workflow_request(
                host_id,
                RemoteWorkflowIntent::Runs(id.to_string()),
                "store_list_runs",
                by_id.clone(),
            );
            self.remote_workflow_request(
                host_id,
                RemoteWorkflowIntent::Notes(id.to_string()),
                "store_list_notes",
                by_id.clone(),
            );
            self.remote_workflow_request(
                host_id,
                RemoteWorkflowIntent::Proposals(id.to_string()),
                "store_list_proposals",
                by_id.clone(),
            );
            self.remote_workflow_request(
                host_id,
                RemoteWorkflowIntent::Revisions(id.to_string()),
                "store_list_revisions",
                by_id,
            );
        }
    }

    /// Once a tick: re-read the selected remote workflow's runs while one is
    /// unsettled.
    ///
    /// Rate-limited by the request dedupe rather than a clock — an answer has
    /// to land before the next ask goes out — which bounds the polling to one
    /// round trip at a time and costs nothing when every run has settled.
    pub fn tick_remote_workflows(&mut self) {
        if self.tab() != "Workflows" {
            return;
        }
        let WorkflowSource::Remote { host_id } = self.workflow_source() else {
            return;
        };
        // Roughly every second at the 90 ms frame rate: a run's steps change
        // on that timescale, and asking every frame would only queue answers.
        if !self.frame.is_multiple_of(11) {
            return;
        }
        let Some(id) = self.selected_workflow().map(|w| w.id.clone()) else {
            return;
        };
        self.poll_remote_workflow_runs(&host_id, &id);
    }

    /// Re-read the selected workflow's runs from the host.
    ///
    /// The one read that is *not* cached-until-invalidated: a run in flight
    /// changes on the host without anything here knowing, so the rail polls
    /// it while one is unsettled.
    fn poll_remote_workflow_runs(&mut self, host_id: &str, id: &str) {
        let cache = self.remote_workflow_cache(host_id);
        let unsettled = cache
            .lock()
            .expect("remote workflow cache")
            .runs
            .get(id)
            .is_some_and(|runs| runs.iter().any(|run| !run.status.is_settled()));
        if unsettled {
            self.remote_workflow_request(
                host_id,
                RemoteWorkflowIntent::Runs(id.to_string()),
                "store_list_runs",
                serde_json::json!({ "id": id }),
            );
        }
    }

    /// Fold a host's answer into its cache, then re-read the tab from it.
    pub(in crate::ui::app) fn fold_remote_workflow_result(
        &mut self,
        host_id: &str,
        request_id: &str,
        result: Result<serde_json::Value, String>,
    ) {
        let cache = self.remote_workflow_cache(host_id);
        let intent = {
            let mut cache = cache.lock().expect("remote workflow cache");
            let Some(intent) = cache.pending.remove(request_id) else {
                return;
            };
            match result {
                Ok(value) => {
                    if let Err(error) = cache.fold(&intent, value) {
                        cache.error = Some(error);
                    } else {
                        cache.error = None;
                    }
                }
                Err(error) => cache.error = Some(error),
            }
            intent
        };
        // Only the tab scoped to this host is looking at the cache; another
        // host's answer arriving late must not reload a tab pointed elsewhere.
        if self.current_host.host_id() != Some(host_id) {
            return;
        }
        match intent {
            RemoteWorkflowIntent::List => self.reload_workflows(),
            RemoteWorkflowIntent::Get(_) => self.reload_workflow_graph(),
            RemoteWorkflowIntent::Runs(_)
            | RemoteWorkflowIntent::Notes(_)
            | RemoteWorkflowIntent::Proposals(_)
            | RemoteWorkflowIntent::Revisions(_) => self.reload_workflow_runs(),
            // A run was admitted (or refused): the answer is a status, and the
            // run list is what shows it.
            RemoteWorkflowIntent::Run(id) => {
                let error = cache.lock().expect("remote workflow cache").error.clone();
                match error {
                    Some(error) => self.set_status(format!("{host_id}: {error}")),
                    None => self.set_status(format!("{host_id}: {id} started")),
                }
                let mut cache = cache.lock().expect("remote workflow cache");
                cache.runs.remove(&id);
                drop(cache);
                self.reload_workflow_runs();
            }
            RemoteWorkflowIntent::Delete(id) => {
                let error = cache.lock().expect("remote workflow cache").error.clone();
                match error {
                    Some(error) => self.set_status(format!("{host_id}: {error}")),
                    None => self.set_status(format!("{host_id}: deleted {id}")),
                }
                self.refresh_remote_workflows(host_id);
            }
        }
    }
}

impl App {
    /// Route a workflow command to the current host.
    ///
    /// On this device the command goes to the event loop untouched. On a
    /// remote host, running, cancelling and deleting become forwarded MCP
    /// calls, and everything that authors — the copilot, a dry run, undo,
    /// evolution — is refused with a status line, because the store over there
    /// is read-only from here by design.
    pub(in crate::ui::app) fn scope_workflow_cmd(&mut self, cmd: Option<Cmd>) -> Option<Cmd> {
        let WorkflowSource::Remote { host_id } = self.workflow_source() else {
            return cmd;
        };
        let refused = |what: &str| format!("{what} is not available on {host_id}; press H to switch to this device");
        match cmd? {
            Cmd::RunWorkflow { id, inputs } => {
                self.set_status(format!("Running {id} on {host_id}…"));
                self.remote_workflow_request(
                    &host_id,
                    RemoteWorkflowIntent::Run(id.clone()),
                    "workflow_run",
                    serde_json::json!({ "id": id, "input": inputs, "wait": "no" }),
                );
                None
            }
            Cmd::DeleteWorkflow { id } => {
                self.remote_workflow_request(
                    &host_id,
                    RemoteWorkflowIntent::Delete(id.clone()),
                    "workflow_delete",
                    serde_json::json!({ "id": id }),
                );
                None
            }
            Cmd::DryRunWorkflow { .. } => {
                self.set_status(refused("A dry run"));
                None
            }
            Cmd::UndoWorkflow { .. } => {
                self.set_status(refused("Undo"));
                None
            }
            Cmd::CopilotTurn { .. } | Cmd::CreateWorkflow { .. } | Cmd::AbortCopilot { .. } => {
                self.set_status(refused("The copilot"));
                None
            }
            Cmd::RepairWorkflow { .. } | Cmd::EvolveWorkflow { .. } => {
                self.set_status(refused("Evolution"));
                None
            }
            Cmd::AcceptProposal { .. } | Cmd::RejectProposal { .. } => {
                self.set_status(refused("Deciding a proposal"));
                None
            }
            other => Some(other),
        }
    }
}
