//! Tests for the Workflows tab on a remote host: the store over the cache,
//! how answers fold in, and what the tab refuses to do remotely.

use std::sync::Arc;

use medulla::config::{LoadedConfig, RemoteHostSection};
use medulla::backend::MockBackend;
use medulla::backend::Backend;
use serde_json::json;

use super::super::super::hosts::HostSelection;
use super::super::super::types::{tab_pos, App, Cmd};
use super::super::tests::diamond;
use super::{RemoteWorkflowIntent, WorkflowSource};

/// An app with one paired host, scoped to it, with a channel standing in for
/// the host's connection so requests can be observed.
fn app_on_remote() -> (
    App,
    tokio::sync::mpsc::UnboundedReceiver<crate::remote::client::RemoteRequest>,
) {
    let rt: Arc<dyn Backend> = Arc::new(MockBackend::new());
    let mut loaded = LoadedConfig::defaults("medulla.tui.json".into());
    loaded.config.remote_hosts = vec![RemoteHostSection {
        host: "gpu.example.com".to_string(),
        udp_port: 60001,
        enabled: true,
        ..RemoteHostSection::default()
    }];
    let mut app = App::new(rt, loaded);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app.remote_host_dialling("gpu-example-com", tx);
    app.remote_host_connected("gpu-example-com", Default::default());
    app.tab_index = tab_pos("Workflows");
    app.set_current_host(HostSelection::Host("gpu-example-com".to_string()));
    (app, rx)
}

fn drain(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::remote::client::RemoteRequest>,
) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    while let Ok(request) = rx.try_recv() {
        if let crate::remote::client::RemoteRequest::WorkflowCall {
            request_id, tool, ..
        } = request
        {
            calls.push((request_id, tool));
        }
    }
    calls
}

#[test]
fn the_source_follows_the_selection() {
    let (mut app, _rx) = app_on_remote();
    assert_eq!(
        app.workflow_source(),
        WorkflowSource::Remote {
            host_id: "gpu-example-com".to_string()
        }
    );
    app.set_current_host(HostSelection::All);
    assert_eq!(app.workflow_source(), WorkflowSource::Local);
    app.set_current_host(HostSelection::ThisDevice);
    assert_eq!(app.workflow_source(), WorkflowSource::Local);
}

#[test]
fn entering_the_tab_asks_the_host_for_its_catalogue_once() {
    let (mut app, mut rx) = app_on_remote();
    app.reload_workflows();
    app.reload_workflows();
    let calls = drain(&mut rx);
    assert_eq!(calls.len(), 1, "one outstanding list at a time: {calls:?}");
    assert_eq!(calls[0].1, "store_list");
    assert!(app.status.contains("(remote)"), "{}", app.status);
}

#[test]
fn a_catalogue_answer_lists_and_fetches_what_the_selection_needs() {
    let (mut app, mut rx) = app_on_remote();
    app.reload_workflows();
    let (list_id, _) = drain(&mut rx).remove(0);
    let record = diamond("sweep");
    let summary = json!([{
        "id": "sweep", "name": "sweep workflow", "description": "", "enabled": true,
        "nodeCount": 5, "triggerKind": "manual", "inputs": []
    }]);
    app.remote_workflow_result("gpu-example-com", &list_id, Ok(summary));
    assert_eq!(app.workflow_row_count(), 1);

    // Selecting fetched the record and its journal.
    let mut tools: Vec<String> = drain(&mut rx).into_iter().map(|(_, tool)| tool).collect();
    tools.sort();
    assert_eq!(
        tools,
        vec![
            "store_get",
            "store_list_notes",
            "store_list_proposals",
            "store_list_revisions",
            "store_list_runs",
        ]
    );

    // Fold the record: the canvas now has a graph.
    let cache = app.remote_workflow_cache("gpu-example-com");
    let get_id = cache
        .lock()
        .unwrap()
        .pending
        .iter()
        .find(|(_, intent)| **intent == RemoteWorkflowIntent::Get("sweep".to_string()))
        .map(|(id, _)| id.clone())
        .unwrap();
    app.remote_workflow_result(
        "gpu-example-com",
        &get_id,
        Ok(serde_json::to_value(&record).unwrap()),
    );
    assert!(app.wf.graph.is_some(), "the remote record drew a graph");
    let store = app.workflow_store();
    assert_eq!(store.get("sweep").unwrap().unwrap().id, "sweep");
    // Writes are refused with the host named.
    let refused = store.save(&record).unwrap_err().to_string();
    assert!(refused.contains("gpu-example-com"), "{refused}");
}

#[test]
fn the_new_row_is_not_selected_while_a_non_empty_catalogue_is_still_loading() {
    // Before the host has answered, an empty read of its cache is not the
    // same as the host confirming an empty store — `reload_workflows` must
    // not treat the two alike, or `creating` latches on the unlisted cache
    // and never clears once the real, non-empty catalogue lands (nothing
    // but a cursor move otherwise resets it).
    let (mut app, mut rx) = app_on_remote();
    app.reload_workflows();
    assert!(
        !app.wf.creating,
        "an unanswered catalogue is not a confirmed-empty one"
    );

    let (list_id, _) = drain(&mut rx).remove(0);
    let summary = json!([{
        "id": "sweep", "name": "sweep workflow", "description": "", "enabled": true,
        "nodeCount": 5, "triggerKind": "manual", "inputs": []
    }]);
    app.remote_workflow_result("gpu-example-com", &list_id, Ok(summary));
    assert!(
        !app.wf.creating,
        "a non-empty catalogue must not leave the New row selected"
    );
}

#[test]
fn refreshing_clears_a_stuck_pending_intent_so_it_can_be_asked_again() {
    let (mut app, mut rx) = app_on_remote();
    app.reload_workflows();
    assert_eq!(drain(&mut rx).len(), 1, "the first ask goes out");

    // No answer ever arrives (a transport hiccup after the request entered
    // `pending`) — a second reload is deduplicated against it, exactly as it
    // should be while it is genuinely still outstanding.
    app.reload_workflows();
    assert!(
        drain(&mut rx).is_empty(),
        "still deduplicated while pending"
    );

    // `r` must not leave that stuck intent blocking a fresh ask forever.
    app.refresh_workflows();
    assert_eq!(
        drain(&mut rx).len(),
        1,
        "refresh must ask again despite the stuck pending intent"
    );
}

#[test]
fn reconnecting_clears_pending_so_a_stale_request_does_not_block_a_fresh_ask() {
    let (mut app, mut rx) = app_on_remote();
    app.reload_workflows();
    assert_eq!(drain(&mut rx).len(), 1, "the first ask goes out");

    // The connection that request was sent on is gone; nothing will ever
    // answer it under the old request id. Reconnecting must not leave it
    // sitting in `pending` blocking the catalogue from ever being asked for
    // again.
    app.remote_host_connected("gpu-example-com", Default::default());
    assert_eq!(
        drain(&mut rx).len(),
        1,
        "reconnecting's own reload must not be deduplicated against the stale request"
    );
}

#[test]
fn a_bad_answer_is_reported_rather_than_folded() {
    let (mut app, mut rx) = app_on_remote();
    app.reload_workflows();
    let (list_id, _) = drain(&mut rx).remove(0);
    app.remote_workflow_result("gpu-example-com", &list_id, Ok(json!({ "not": "a list" })));
    let cache = app.remote_workflow_cache("gpu-example-com");
    let error = cache.lock().unwrap().error.clone();
    assert!(
        error.as_deref().is_some_and(|e| e.contains("catalogue")),
        "{error:?}"
    );
    // And an answer nobody asked for is ignored.
    app.remote_workflow_result("gpu-example-com", "wf-bogus-9", Ok(json!([])));
}

#[test]
fn running_goes_over_the_link_and_authoring_is_refused() {
    let (mut app, mut rx) = app_on_remote();
    // Selecting the host already asked for the catalogue.
    assert_eq!(drain(&mut rx)[0].1, "store_list");
    let cmd = app.scope_workflow_cmd(Some(Cmd::RunWorkflow {
        id: "sweep".to_string(),
        inputs: Default::default(),
    }));
    assert!(
        cmd.is_none(),
        "the run went to the host, not the event loop"
    );
    let calls = drain(&mut rx);
    assert_eq!(calls[0].1, "workflow_run");

    let cmd = app.scope_workflow_cmd(Some(Cmd::DryRunWorkflow {
        id: "sweep".to_string(),
        inputs: Default::default(),
    }));
    assert!(cmd.is_none());
    assert!(
        app.status.contains("not available on gpu-example-com"),
        "{}",
        app.status
    );
    assert!(
        drain(&mut rx).is_empty(),
        "nothing was sent for a refused action"
    );

    // Off the remote host, commands pass through untouched.
    app.set_current_host(HostSelection::All);
    let cmd = app.scope_workflow_cmd(Some(Cmd::DryRunWorkflow {
        id: "sweep".to_string(),
        inputs: Default::default(),
    }));
    assert!(matches!(cmd, Some(Cmd::DryRunWorkflow { .. })));
}
