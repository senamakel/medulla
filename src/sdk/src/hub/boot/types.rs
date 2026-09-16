//! Data types for the `boot` module.
#[allow(unused_imports)]
use super::*;

use medulla_link::keys::{NodeId, PairKey};

/// One enrolled peer on the host link.
///
/// Carries all three identifiers because they come from different places and
/// none is derivable from the others: the name is the registry's, the id is the
/// backend's, and the pair key is the user's (protocol §7.1) and never passed
/// through an API.
#[derive(Debug, Clone)]
pub struct HubLinkPeer {
    /// The bridge address callers use.
    pub name: String,
    /// The 16-byte identifier that travels on the wire.
    pub node_id: NodeId,
    /// The end-to-end key shared with this peer.
    pub pair_key: PairKey,
}

/// How the hub brings up its host link.
#[derive(Clone)]
pub struct HubLinkConfig {
    /// The link identity directory, normally `<medulla_home>/link`.
    pub state_dir: PathBuf,
    /// This endpoint's node name — the address peers send to.
    pub node_name: String,
    /// Overrides the forwarder endpoint recorded in `node.json`.
    pub forwarder_endpoint: Option<String>,
    /// The hosts this orchestrator has enrolled.
    pub peers: Vec<HubLinkPeer>,
    /// An already-open link owned by the embedding process.
    ///
    /// Embedded hubs share this handle with status observation because the
    /// identity directory is exclusively locked. Standalone hubs leave it
    /// unset and open the link during startup.
    pub handle: Option<Arc<medulla_link::LinkHandle>>,
}

/// One worker the hub fronts on the backend roster.
///
/// A projection of an [`AgentDeclaration`](crate::fleet::AgentDeclaration) for
/// a host in this process, and of a remembered roster row for a remote peer.
/// Several specs may share one `address`: a machine advertises one entry per
/// declared agent, and `id` is what tells them apart.
#[derive(Debug, Clone, Default)]
pub struct WorkerSpec {
    /// The `agentId` the backend targets (defaults to the worker's node name).
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
    /// Its path is advertised as `metadata.workspace`; see
    /// [`HubWorker::workspace`](crate::hub::HubWorker::workspace).
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
    /// The host link to bring up, or `None` for a device-local-only hub.
    pub link: Option<HubLinkConfig>,
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
    /// The device-local bus this hub also dispatches over.
    ///
    /// `Some` makes the hub bi-modal: a worker whose address is bound on this
    /// network is reached in-process, and everything else still goes over the
    /// host link. That is what lets one program be both the dispatcher and a
    /// host for its own machine. `None` keeps every worker remote.
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
