//! Roster and worker-registry data types shared by the hub, the protocol
//! service, and the UI.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A connected agent medulla can delegate to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentDescriptor {
    /// Stable roster identity.
    pub id: String,
    /// Model-facing display name.
    #[serde(default)]
    pub name: String,
    /// Prompt-facing description of the agent's role.
    #[serde(default)]
    pub description: String,
    /// Open-vocabulary liveness; only `offline` prevents delegation.
    #[serde(default)]
    pub availability: String,
    /// The workspace this harness-backed agent is deployed into.
    #[serde(
        default,
        rename = "workspaceId",
        skip_serializing_if = "Option::is_none"
    )]
    pub workspace_id: Option<String>,
    /// The host for a local agent that has no workspace parent.
    #[serde(default, rename = "hostId", skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    /// Provisioning template provenance, when the agent was spawned from one.
    #[serde(
        default,
        rename = "templateId",
        skip_serializing_if = "Option::is_none"
    )]
    pub template_id: Option<String>,
    /// Model-facing capability tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Opaque harness-owned metadata, never interpreted by this client.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// Latest liveness reading for one roster agent (remote transports only).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentPresence {
    /// Whether the agent is currently considered reachable.
    pub online: bool,
    /// Optional provider- or transport-specific status detail.
    pub detail: Option<String>,
    /// Timestamp of the liveness observation.
    pub at: i64,
}

/// One wrapper session on a peer machine, as shown in the Agents view.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerSession {
    /// Stable wrapper-session identifier.
    pub id: String,
    /// Open-vocabulary session state.
    pub state: String,
    /// Harness serving the session, when reported.
    pub harness: Option<String>,
    /// Timestamp of the most recent observation.
    pub last_seen_at: i64,
}

/// One managed worker peer, projected from a `worker.list` entry. A worker is a
/// remote host-link peer the dispatcher can delegate to. §4.2 is load-bearing:
/// `id` is the registry's own stable handle (for select/edit/remove), `address` is
/// the messaging target, and `peer_id` (the wallet) is a separate field — never
/// merged.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerInfo {
    /// Stable registry identifier used by worker mutations.
    pub id: String,
    /// Messaging address used to reach the worker.
    pub address: String,
    /// Optional link handle.
    pub handle: Option<String>,
    /// Optional operator-defined label.
    pub label: Option<String>,
    /// Default coding-agent harness, when known.
    pub harness: Option<String>,
    /// Absolute path this worker runs tasks in, when the hub knows it — the
    /// device-local host reports its own; a remote peer's is unknown until it
    /// answers a capability probe.
    pub workspace: Option<String>,
    /// Wallet or peer identity, distinct from the worker registry identifier.
    pub peer_id: Option<String>,
    /// Logical CPU cores reported by the worker.
    pub cpu_cores: Option<u32>,
    /// Total physical memory reported by the worker, in bytes.
    pub memory_total_bytes: Option<u64>,
    /// Currently available memory reported by the worker, in bytes.
    pub memory_available_bytes: Option<u64>,
    /// Primary IPv4 address reported by the worker.
    pub ip_address: Option<String>,
    /// Whether this worker is the current manual default.
    pub selected: bool,
    /// Per-harness token budgets the worker advertised on its capability probe.
    /// Empty when none were reported. Display-only; the dispatcher sizes tasks.
    pub budgets: Vec<crate::protocol::HarnessBudget>,
    /// Per-harness readiness the worker advertised on its capability probe. Empty
    /// when none were reported. Display-only.
    pub readiness: Vec<crate::protocol::HarnessReadiness>,
}

/// A mutation on the worker-peer registry (`worker.add`/`select`/`update`/`remove`).
#[derive(Debug, Clone)]
pub enum WorkerOp {
    /// Add a worker by address or handle.
    Add {
        /// Direct messaging address, mutually exclusive with `handle`.
        address: Option<String>,
        /// link handle, mutually exclusive with `address`.
        handle: Option<String>,
        /// Optional operator-facing label.
        label: Option<String>,
        /// Optional default harness hint.
        harness: Option<String>,
    },
    /// Select a worker as the manual default.
    Select {
        /// Registry identifier of the worker.
        id: String,
    },
    /// `patch` is a JSON object of the fields to change (e.g. `{"label": "..."}`); an
    /// empty-string value clears an optional field, mirroring `worker.update`.
    Update {
        /// Registry identifier of the worker.
        id: String,
        /// Partial worker fields to replace.
        patch: Map<String, Value>,
    },
    /// Remove a worker from the registry.
    Remove {
        /// Registry identifier of the worker.
        id: String,
    },
    /// Ask a worker for current CPU, RAM, and IP details.
    RefreshDetails {
        /// Registry identifier of the worker to probe.
        id: String,
    },
}
