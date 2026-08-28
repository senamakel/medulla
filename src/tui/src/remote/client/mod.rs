//! The client half: dialling a remote host and driving its sessions.
//!
//! One connection per host, shared by every session on it, and opened the first
//! time a session is actually wanted rather than at startup. That is why the
//! link's peer table is fixed at one entry and why sessions are multiplexed over
//! channel 0 — the expensive thing is the SSH bootstrap, and it happens once per
//! machine, not once per shell.
//!
//! The screen is the exception, and not by choice: channel 1 carries a single
//! synchronised grid per peer, so exactly one session's *pixels* stream at a
//! time. [`RemoteHost::watch`] moves that stream. Every other session stays live
//! and keeps appearing in the session list, which rides the reliable channel.

mod bootstrap;
pub mod exec;
pub mod task;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use medulla::bridge::{Bridge, LinkBridge, LinkBridgeConfig, LinkPeer};
use medulla::config::RemoteHostSection;
use medulla::protocol::{
    apply_frame, encode_remote_message, encode_screen_message, parse_remote_message,
    parse_screen_message, ApplyOutcome, RemoteCapabilities, RemoteMessage, RemoteSessionRow,
    ScreenMessage, ScreenView,
};
use medulla_link::keys::{self, ForwarderKey, NodeId, NodeState, PairKey, Role};
use medulla_link::{Link, LinkConfig, LinkPath, PeerConfig};

pub use bootstrap::{bootstrap, remote_command, ssh_argv, BootstrapError, Bootstrapped};
pub use task::{RemoteRequest, RemoteUpdate};

use crate::worker::pty::ScreenSnapshot;
use crate::worker::stream::snapshot_from_grid;

/// The bridge address the daemon is known by on this side.
const HOST_ADDRESS: &str = "medulla-host";

/// One connected remote machine.
pub struct RemoteHost {
    /// Stable id from `[[remoteHosts]]`, and the lane sessions are filed under.
    pub id: String,
    bridge: Arc<dyn Bridge>,
    /// What the host said it can do.
    capabilities: RemoteCapabilities,
    /// Every session over there, as last reported.
    rows: Vec<RemoteSessionRow>,
    /// The session whose screen is streaming, and the view being folded.
    watching: Option<(String, Option<ScreenView>)>,
    /// Held so the SSH session is not reaped while the host is in use.
    _ssh: Option<tokio::process::Child>,
}

impl RemoteHost {
    /// Bootstrap `host` over SSH and bring the direct link up.
    ///
    /// `state_dir` must be unique per host: `keys::acquire` takes an advisory
    /// lock for the life of the link, so two hosts sharing a directory would
    /// leave the second unable to start.
    ///
    /// # Errors
    ///
    /// A [`BootstrapError`] rendered to a string, or a link that could not be
    /// brought up. Both are terminal for this host and neither is retried here —
    /// the caller decides whether to offer that.
    pub async fn connect(
        id: &str,
        host: &RemoteHostSection,
        state_dir: &std::path::Path,
        workspace: &str,
    ) -> Result<Self, String> {
        let client = mint_identity(state_dir)?;
        let started = bootstrap(host, client, workspace)
            .await
            .map_err(|error| error.to_string())?;
        enroll_host(
            state_dir,
            started.connect.node_id,
            &started.connect.pair_key,
        )?;

        let mut config = LinkConfig::new(state_dir);
        config.path = LinkPath::Direct {
            endpoint: started.endpoint.clone(),
        };
        config.peers = vec![PeerConfig {
            node_id: started.connect.node_id,
            pair_key: started.connect.pair_key.clone(),
        }];
        let link = Link::connect(config).await.map_err(|e| e.to_string())?;
        let bridge = Arc::new(LinkBridge::new(
            Arc::new(link),
            LinkBridgeConfig {
                node_name: "medulla-client".to_string(),
                peers: vec![LinkPeer {
                    name: HOST_ADDRESS.to_string(),
                    node_id: started.connect.node_id,
                }],
            },
        ));
        Ok(RemoteHost {
            id: id.to_string(),
            bridge,
            capabilities: RemoteCapabilities::default(),
            rows: Vec::new(),
            watching: None,
            _ssh: Some(started.child),
        })
    }

    /// Build a host over an already-connected bridge.
    ///
    /// The seam the tests use, and the one an embedding would use to supply its
    /// own transport.
    pub fn over(id: &str, bridge: Arc<dyn Bridge>) -> Self {
        RemoteHost {
            id: id.to_string(),
            bridge,
            capabilities: RemoteCapabilities::default(),
            rows: Vec::new(),
            watching: None,
            _ssh: None,
        }
    }

    /// What the host said it can do. Empty until [`hello`](Self::hello) is
    /// answered.
    pub fn capabilities(&self) -> &RemoteCapabilities {
        &self.capabilities
    }

    /// Every session on the host, as last reported.
    pub fn rows(&self) -> &[RemoteSessionRow] {
        &self.rows
    }

    /// The session whose screen is streaming.
    pub fn watching(&self) -> Option<&str> {
        self.watching.as_ref().map(|(id, _)| id.as_str())
    }

    /// The watched session's screen, ready for the same renderer a local pane
    /// uses.
    pub fn screen(&self) -> Option<ScreenSnapshot> {
        self.watching
            .as_ref()
            .and_then(|(_, view)| view.as_ref())
            .map(|view| snapshot_from_grid(&view.grid))
    }

    /// Announce ourselves and ask what this host can do.
    pub async fn hello(&self) -> Result<(), String> {
        self.send(&encode_remote_message(&RemoteMessage::Hello {
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        }))
        .await
    }

    /// Ask for a session, by the id of a harness this host advertised.
    pub async fn open(
        &self,
        request_id: &str,
        harness_id: &str,
        workspace: &str,
        cols: u16,
        rows: u16,
        name: Option<String>,
    ) -> Result<(), String> {
        let harness = self
            .capabilities
            .harnesses
            .iter()
            .find(|choice| choice.id == harness_id)
            .cloned()
            .ok_or_else(|| format!("{} does not offer {harness_id}", self.id))?;
        self.send(&encode_remote_message(&RemoteMessage::Open {
            request_id: request_id.to_string(),
            harness,
            workspace: workspace.to_string(),
            cols,
            rows,
            name,
        }))
        .await
    }

    /// Stream `session_id`'s screen, replacing whatever was streaming before.
    pub async fn watch(&mut self, session_id: &str) -> Result<(), String> {
        if let Some((previous, _)) = &self.watching {
            if previous != session_id {
                let body = encode_screen_message(&ScreenMessage::Unsubscribe {
                    task_id: previous.clone(),
                });
                self.send(&body).await?;
            }
        }
        // `None` rather than an empty view: `apply_frame` owns the
        // "nothing held yet" case, and it is what makes the first frame
        // required to be a full one.
        self.watching = Some((session_id.to_string(), None));
        self.send(&encode_screen_message(&ScreenMessage::Subscribe {
            task_id: session_id.to_string(),
            max_fps: 10,
            resync: true,
        }))
        .await
    }

    /// Send keystrokes to the watched session.
    ///
    /// Encoded here, as bytes, because this terminal is the one that knows its
    /// own modes — the far side is told what was typed, not asked to work it out.
    pub async fn input(&self, bytes: &[u8]) -> Result<(), String> {
        let Some((session_id, _)) = &self.watching else {
            return Err("nothing is being watched".to_string());
        };
        self.send(&encode_screen_message(&ScreenMessage::input(
            session_id, bytes,
        )))
        .await
    }

    /// Tell the host the pane's size, so its pty reflows to match.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let Some((session_id, _)) = &self.watching else {
            return Ok(());
        };
        self.send(&encode_screen_message(&ScreenMessage::Resize {
            task_id: session_id.clone(),
            cols,
            rows,
        }))
        .await
    }

    /// End a session on the host.
    pub async fn close(&self, session_id: &str) -> Result<(), String> {
        self.send(&encode_remote_message(&RemoteMessage::Close {
            session_id: session_id.to_string(),
        }))
        .await
    }

    /// Drain whatever has arrived and fold it in.
    ///
    /// Returns what happened to any `Open` request answered in this pass — a
    /// session that started, or one the host refused and said why. Before
    /// `OpenFailed` had a variant here, its `reason` was parsed off the wire and
    /// dropped: the daemon told the caller exactly why the harness could not
    /// start (an invalid directory, a missing binary) and nothing ever repeated
    /// it, so the operator saw only a generic "never confirmed the session"
    /// after a 30s wait.
    pub async fn pump(&mut self) -> Vec<PumpEvent> {
        let mut events = Vec::new();
        for message in self.bridge.drain_inbox(64).await {
            if let Some(remote) = parse_remote_message(&message.text) {
                match remote {
                    RemoteMessage::Capabilities(capabilities) => self.capabilities = capabilities,
                    RemoteMessage::Sessions { rows } => self.rows = rows,
                    RemoteMessage::Opened { session_id, .. } => {
                        events.push(PumpEvent::Opened(session_id));
                    }
                    RemoteMessage::OpenFailed { reason, .. } => {
                        events.push(PumpEvent::OpenFailed(reason));
                    }
                    RemoteMessage::Hello { .. }
                    | RemoteMessage::Open { .. }
                    | RemoteMessage::Close { .. } => {}
                }
                continue;
            }
            if let Some(ScreenMessage::Frame(frame)) = parse_screen_message(&message.text) {
                self.apply(frame).await;
            }
        }
        events
    }

    /// Fold one frame into the watched view.
    async fn apply(&mut self, frame: medulla::protocol::ScreenFrame) {
        let Some((session_id, view)) = &mut self.watching else {
            return;
        };
        if frame.task_id != *session_id {
            // A frame for the session we just stopped watching. Harmless, and
            // expected: the host may already have had one in flight.
            return;
        }
        if apply_frame(view, &frame) == ApplyOutcome::NeedsResync {
            // Not retried — the frame cannot be applied and never will be. A
            // fresh full frame supersedes everything missed, which is the whole
            // recovery path.
            let body = encode_screen_message(&ScreenMessage::Subscribe {
                task_id: session_id.clone(),
                max_fps: 10,
                resync: true,
            });
            let _ = self.bridge.send(HOST_ADDRESS, &body).await;
        }
    }

    async fn send(&self, body: &str) -> Result<(), String> {
        self.bridge.send(HOST_ADDRESS, body).await
    }
}

/// Mint this client's identity for one host, returning its node id.
///
/// A fresh identity per host, in its own directory. Reusing one would mean a
/// single advisory lock shared by every connection, so only the first host would
/// come up.
fn mint_identity(dir: &std::path::Path) -> Result<NodeId, String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let node = keys::acquire_or_create(dir, || NodeState {
        version: 1,
        node_id: NodeId::generate(),
        // The orchestrator half of the direction bit (§4.2): this end dialled
        // in, the daemon is the host. The two halves must disagree, or both
        // would draw nonces from the same space under one key.
        role: Role::Orchestrator,
        pair_key: PairKey::generate(),
        forwarder_key: ForwarderKey::generate(),
        forwarder_endpoint: String::new(),
        peer_node_id: NodeId::generate(),
        peers: Vec::new(),
        seq_reservation: 1,
    })
    .map_err(|error| error.to_string())?;
    let node_id = node.state.node_id;
    drop(node);
    Ok(node_id)
}

/// Record the key the host minted for us, so the link can be brought up on it.
fn enroll_host(dir: &std::path::Path, host: NodeId, pair_key: &PairKey) -> Result<(), String> {
    let path = keys::node_path(dir);
    // Read from disk rather than from a held handle, for the same reason the
    // daemon's enrollment does: the sequence reservation on disk is ahead of any
    // in-memory copy, and writing a stale one back would rewind it.
    let mut state = keys::read_node_state(&path).map_err(|error| error.to_string())?;
    state.peers.retain(|peer| peer.node_id != host);
    state.peers.push(medulla_link::keys::EnrolledPeer {
        node_id: host,
        pair_key: pair_key.clone(),
    });
    state.peer_node_id = host;
    state.pair_key = pair_key.clone();
    keys::write_node_state(&path, &state).map_err(|error| error.to_string())
}
