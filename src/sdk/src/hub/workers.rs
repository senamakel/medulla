//! The hub as the UI sees it: the roster as [`WorkerInfo`] rows, and the
//! [`WorkerOp`] mutations the Hosts page applies to it.
//!
//! The mapping is a property of the hub's roster shape and the UI's row type,
//! not of whoever is asking, so it lives beside the hub rather than in the app
//! crate. [`HubSlot`] is the one place a live [`HubHandle`] is shared with the
//! surfaces that read it on the render path.

use std::sync::{Arc, Mutex};

use anyhow::anyhow;

use super::{HubHandle, HubWorker};
use crate::fleet::{WorkerInfo, WorkerOp};

/// Shared slot the hub fills with its handle once it is up.
///
/// A `std::sync::Mutex` because every read happens on the render path, which
/// must not await; the guard is never held across one.
pub type HubSlot = Arc<Mutex<Option<HubHandle>>>;

/// The live handle in `slot`, if the hub has started.
pub fn hub_in(slot: &HubSlot) -> Option<HubHandle> {
    slot.lock().ok().and_then(|guard| guard.clone())
}

/// The roster as UI rows, with whatever system details the hub has probed.
pub fn workers_of(slot: &HubSlot) -> Vec<WorkerInfo> {
    let Some(hub) = hub_in(slot) else {
        return Vec::new();
    };
    hub.list()
        .into_iter()
        .map(|worker| {
            let details = hub.system_info(&worker.id);
            hub_worker_to_info(worker, details)
        })
        .collect()
}

/// Map a hub roster entry to the UI's [`WorkerInfo`] row. An `@handle` address
/// is surfaced as the `handle` field too.
pub fn hub_worker_to_info(
    worker: HubWorker,
    details: Option<crate::protocol::WorkerSystemInfo>,
) -> WorkerInfo {
    let (cpu_cores, memory_total_bytes, memory_available_bytes, ip_address) = details
        .map(|details| {
            (
                Some(details.cpu_cores),
                details.memory_total_bytes,
                details.memory_available_bytes,
                Some(details.ip_address),
            )
        })
        .unwrap_or_default();
    // The path only: the UI row has always been a path, and the workspace
    // *type* has no renderer yet. Read through `workspace_path` rather than off
    // the field, so a blank or space-padded declaration is *absent* here — a
    // row that instead rendered "  " would show the agent working somewhere.
    let workspace = worker.workspace_path().map(str::to_owned);
    WorkerInfo {
        handle: worker
            .address
            .starts_with('@')
            .then(|| worker.address.clone()),
        id: worker.id,
        workspace,
        address: worker.address,
        label: worker.label,
        harness: Some(worker.harness),
        peer_id: None,
        cpu_cores,
        memory_total_bytes,
        memory_available_bytes,
        ip_address,
        selected: worker.selected,
        // Budgets/readiness ride the capability probe, not the roster mapping;
        // the hub projection carries none, so they default empty here.
        budgets: Vec::new(),
        readiness: Vec::new(),
    }
}

/// Translate a [`WorkerOp`] into a hub-handle mutation.
pub async fn apply_worker_op(handle: &HubHandle, op: WorkerOp) -> anyhow::Result<()> {
    match op {
        WorkerOp::Add {
            address,
            handle: named_handle,
            label,
            harness,
        } => {
            let address = address
                .or(named_handle)
                .ok_or_else(|| anyhow!("a worker needs an address or @handle"))?;
            handle
                .add(HubWorker {
                    id: address.clone(),
                    address,
                    harness: harness.unwrap_or_else(|| "claude".to_string()),
                    label,
                    selected: false,
                    // A peer added by address: this hub has no idea where it
                    // runs tasks until it answers a capability probe.
                    workspace: None,
                    ..Default::default()
                })
                .await
        }
        WorkerOp::Remove { id } => handle.remove(&id).await,
        WorkerOp::RefreshDetails { id } => handle.refresh_system_info(&id).await,
        WorkerOp::Update { id, patch } => {
            let label = label_from_patch(&patch)?;
            handle.set_label(&id, label).await
        }
        WorkerOp::Select { id } => {
            handle.select(&id);
            Ok(())
        }
    }
}

/// Validate the only supported worker patch, preserving `""` as explicit clear.
pub fn label_from_patch(
    patch: &serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<Option<String>> {
    let label = patch
        .get("label")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("worker update requires a string label"))?;
    Ok((!label.is_empty()).then(|| label.to_string()))
}
