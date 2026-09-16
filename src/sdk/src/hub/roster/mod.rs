//! The hub's worker-roster data: the shared roster type and the rules that keep
//! one peer to one slot. Pure and offline-testable; the live control handle that
//! mutates the roster lives in [`handle`](super::handle).
//!
//! The roster is shared (`Arc<Mutex<_>>`) between the dispatch path — which
//! reads it to resolve a task's target — and the
//! [`HubHandle`](super::handle::HubHandle) the TUI holds to add/remove workers at
//! runtime.

use std::sync::{Arc, Mutex};

/// The roster entry a [`WorkerSpec`](crate::hub::WorkerSpec) describes.
///
/// The one place a spec becomes a live roster row, so the declaration → advert
/// chain has a single seam: an agent's workspace and derived capacity arrive
/// here from what the operator declared, and `to_agent` turns them into
/// `metadata`.
///
/// Lives here rather than beside the hub's boot wiring because it is pure data
/// translation, and because everything downstream of it is tested here.
pub(crate) fn worker_from_spec(spec: &crate::hub::WorkerSpec) -> HubWorker {
    HubWorker {
        id: spec.id.clone(),
        host_id: spec.host_id.clone(),
        address: spec.address.clone(),
        harness: spec.harness.clone(),
        // The placeholder name is not a label. It is what an env-seeded or
        // remembered row carries when nobody named it, and promoting it to a
        // label would put a constant on screen where the id belongs.
        label: (spec.name != "medulla-worker").then(|| spec.name.clone()),
        selected: false,
        workspace: spec.workspace.clone(),
        // A spec that states no capacity is a remembered roster row or a remote
        // peer, not a declaration; the serial default is the safe reading.
        max_sessions: if spec.max_sessions == 0 {
            crate::fleet::WorkspaceStrategy::Checkout.max_sessions()
        } else {
            spec.max_sessions
        },
        ..Default::default()
    }
}

/// Whether two roster entries name the same destination.
///
/// Blank never matches: an entry with no address is not "the same peer" as
/// another entry with no address.
pub(super) fn same_destination(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && a == b
}

/// Drop every entry that names the same worker as `incoming`.
///
/// Matched on **address as well as id**, because the address is the peer's
/// wallet and the actual delegation target. Two entries differing only in id are
/// two names for one destination: the backend would be advertised the same
/// worker twice, and [`address_of`] could resolve a task to either of them.
///
/// Ids diverge easily in practice — `MEDULLA_HUB_WORKERS="alpha=<addr>"` seeds
/// `alpha`, while adding the same address in the TUI uses the address itself,
/// and an `@handle` differs from the cryptoId it resolves to.
///
/// Returns the removed ids so callers can invalidate state cached by roster id.
pub(super) fn remove_conflicting(
    workers: &mut Vec<HubWorker>,
    incoming: &HubWorker,
) -> Vec<String> {
    let mut removed_ids = Vec::new();
    workers.retain(|worker| {
        let conflicts =
            worker.id == incoming.id || same_destination(&worker.address, &incoming.address);
        if conflicts {
            removed_ids.push(worker.id.clone());
        }
        !conflicts
    });
    removed_ids
}

/// A short, stable, human-scale id for a worker.
///
/// The id is what the orchestrator must reproduce to address this worker: it is
/// rendered first in `agent_list` (`id (name)`) and copied into a task's
/// `agentId`. A 44-character base58 cryptoId reads as noise beside a memorable
/// name, and the model reaches for the name — which then fails validation as an
/// unknown agent. Making the id the memorable token removes the wrong answer
/// instead of catching it.
///
/// The cryptoId is not lost: it stays the `address`, and is advertised in the
/// descriptor's metadata.
pub(crate) fn worker_id(label: Option<&str>, harness: &str, taken: &[String]) -> String {
    let base = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(slug)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}-worker", slug(harness)));
    let base = if base.is_empty() {
        "worker".to_string()
    } else {
        base
    };
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    // Two workers on one harness with no labels is ordinary; ids must still be
    // distinct or one would shadow the other in the backend's registry.
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.iter().any(|t| t == candidate))
        .expect("an unbounded search always terminates")
}

/// Lowercase, hyphen-separated, alphanumeric — safe to type and to round-trip.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

mod types;
pub use types::SharedRoster;
pub use types::{HubWorker, SharedLocalHosts};
