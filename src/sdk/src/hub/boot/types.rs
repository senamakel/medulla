//! Data types for the `boot` module.
#[allow(unused_imports)]
use super::*;

/// One worker in the hub's roster.
///
/// A projection of an [`AgentDeclaration`](crate::fleet::AgentDeclaration) for
/// a host in this process, and of a remembered roster row for a remote peer.
/// Several specs may share one `address`: a machine lists one entry per
/// declared agent, and `id` is what tells them apart.
#[derive(Debug, Clone, Default)]
pub struct WorkerSpec {
    /// The `agentId` a dispatch targets (defaults to the worker's node name).
    pub id: String,
    /// The host this worker runs on. Empty when the hub has no opinion — a
    /// remote peer the operator added by address.
    pub host_id: String,
    /// The worker's bridge address — its link node name, or a device-local name.
    pub address: String,
    /// Display name for the roster entry.
    pub name: String,
    /// Free-text description / capability summary.
    pub description: String,
    /// The coding-agent harness the worker runs (`claude`/`codex`/`opencode`).
    pub harness: String,
    /// The workspace this worker runs tasks in, when the hub knows it — which is
    /// the case for a host in this same process, and not for a remote host the
    /// operator merely named.
    ///
    /// See [`HubWorker::workspace`](crate::hub::HubWorker::workspace).
    pub workspace: Option<crate::fleet::WorkspaceRef>,
    /// Sessions this worker may run at once, derived from its declared
    /// strategy. Zero means unstated and is normalised to the serial default
    /// when the roster entry is built.
    pub max_sessions: u32,
}
/// Everything [`start_hub`] needs to reach its workers.
/// Not `Debug`: the log sink is a boxed closure with no useful representation.
#[derive(Clone)]
pub struct HubConfig {
    /// The workers in the roster initially (may be empty; add more at runtime).
    pub workers: Vec<WorkerSpec>,
    /// The hosts *this machine* declares, used to decide which roster agents
    /// run locally.
    ///
    /// Shared rather than owned so a host started mid-session joins the same
    /// list the hub reads (see
    /// [`SharedLocalHosts`](crate::hub::SharedLocalHosts)). Default-empty is the
    /// honest answer for a hub that hosts nothing itself: every agent it fronts
    /// then belongs to another machine, and the block says so.
    pub local_hosts: crate::hub::SharedLocalHosts,
    /// How often the runner drains the inbox.
    pub poll: Duration,
    /// Where diagnostics go. Defaults to stderr; a TUI supplies its own so the
    /// hub never writes over a screen it does not own.
    pub log: super::super::types::HubLog,
    /// Where roster changes are saved. `None` keeps the roster in memory only.
    pub persist: Option<super::super::types::RosterSink>,
    /// The device-local bus this hub dispatches over.
    ///
    /// A worker whose address is bound on this network is reached in-process.
    /// That is what lets one program be both the dispatcher and a host for its
    /// own machine. `None` is a configuration error: the hub has no other
    /// transport.
    pub local_network: Option<crate::bridge::LocalBridgeNetwork>,
    /// The address the hub binds on the local bus. Ignored without a
    /// [`local_network`](Self::local_network); empty falls back to
    /// [`DEFAULT_LOCAL_HUB_ADDRESS`](super::DEFAULT_LOCAL_HUB_ADDRESS).
    pub local_address: String,
}
/// A running hub: the live [`HubHandle`] plus the runner kept alive for the
/// session (dropping this stops the pump).
pub struct HubSession {
    /// Live roster control (add/remove/list workers).
    pub handle: HubHandle,
    pub(super) _runner: Arc<TaskRunner>,
}
