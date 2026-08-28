//! Bringing a remote host up, and telling the client how to reach it.
//!
//! This is mosh's bootstrap, and it is deliberately the same shape. The SSH
//! channel is already authenticated and encrypted, so it is the right place —
//! and the only place we get for free — to hand over a fresh key. One line goes
//! to stdout, SSH closes, and everything after that is UDP straight to this
//! process.
//!
//! ```text
//!   MEDULLA-CONNECT v1 port=<u16> node=<32 hex> key=<XXXX-XXXX-…>
//! ```
//!
//! # The key is minted here, never passed in
//!
//! The client never sends a secret to the far side; the far side mints one and
//! sends it back up the SSH pipe. That is not a stylistic choice: a key passed
//! as an argument would appear in this machine's `ps` output for every user on
//! it, for as long as the daemon ran. The node id and port do travel in argv,
//! because neither is a secret — a node id rides in cleartext outer headers by
//! design.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use medulla_link::keys::{self, EnrolledPeer, ForwarderKey, NodeId, NodeState, PairKey, Role};

/// The sentinel a client scans stdout for.
///
/// Scanned for rather than expected on the first line, because a login shell
/// prints whatever it likes first — a MOTD, a `Last login:` banner, `stty`
/// noise — and none of that is an error.
pub const CONNECT_SENTINEL: &str = "MEDULLA-CONNECT v1";

/// Where a direct-link identity lives, given a medulla home.
///
/// Deliberately *not* `<home>/link`, which the enrolled forwarder identity uses:
/// `keys::acquire` takes an advisory lock for the life of the process, so a
/// machine running both a normal `medulla` and this would deadlock on one
/// directory. They are also different identities — one is enrolled with the
/// backend, one is minted here — and sharing a file would let a re-bootstrap
/// clobber an enrollment.
pub fn remote_link_dir(home: &Path) -> PathBuf {
    home.join("remote").join("link")
}

/// What a bootstrapped host tells its client.
#[derive(Debug, Clone)]
pub struct ConnectLine {
    /// The UDP port this daemon is listening on.
    pub port: u16,
    /// This daemon's node id.
    pub node_id: NodeId,
    /// The pair key minted for this client.
    pub pair_key: PairKey,
}

impl ConnectLine {
    /// Render the line for stdout.
    ///
    /// The pair key uses [`PairKey::encode`] — the checksummed, human-carriable
    /// form — so a line mangled by a terminal fails the checksum rather than
    /// producing a key that silently decrypts nothing.
    pub fn render(&self) -> String {
        format!(
            "{CONNECT_SENTINEL} port={} node={} key={}",
            self.port,
            self.node_id,
            self.pair_key.encode()
        )
    }

    /// Parse a line the far side printed, or `None` when this is not one.
    ///
    /// Tolerant of anything before the sentinel on the same line, and of field
    /// order, because what produced it is a login shell rather than a protocol.
    pub fn parse(line: &str) -> Option<Self> {
        let start = line.find(CONNECT_SENTINEL)?;
        let mut port = None;
        let mut node_id = None;
        let mut pair_key = None;
        for field in line[start + CONNECT_SENTINEL.len()..].split_whitespace() {
            let (name, value) = field.split_once('=')?;
            match name {
                "port" => port = value.parse::<u16>().ok(),
                "node" => node_id = NodeId::from_hex(value),
                "key" => pair_key = PairKey::decode(value).ok(),
                // Unknown fields are skipped rather than refused, so a newer
                // daemon may add one without breaking an older client.
                _ => {}
            }
        }
        Some(ConnectLine {
            port: port?,
            node_id: node_id?,
            pair_key: pair_key?,
        })
    }
}

/// Open or mint this host's direct-link identity and enroll `client` on it.
///
/// The identity persists across bootstraps so a returning client talks to a host
/// whose node id has not changed. The *pair key* is fresh every time: keys are
/// per client by construction, and a second client must not inherit the first
/// one's.
///
/// # Errors
///
/// When the identity directory cannot be created, opened, or locked — the last
/// meaning another daemon already holds it, which the caller handles by reusing
/// that instance rather than starting a second one.
pub fn enroll_client(
    dir: &Path,
    client: NodeId,
) -> Result<(NodeId, PairKey), medulla_link::keys::KeyError> {
    std::fs::create_dir_all(dir)?;
    let pair_key = PairKey::generate();
    let node = keys::acquire_or_create(dir, || NodeState {
        version: 1,
        node_id: NodeId::generate(),
        // The host half of the direction bit (§4.2). The client that dialled in
        // is the orchestrator, which is what makes the two nonce spaces
        // disjoint.
        role: Role::Host,
        pair_key: pair_key.clone(),
        // Never used on this path — the direct link derives its outer key from
        // the pair key instead — but the file's schema has the field, and
        // leaving it zeroed would be indistinguishable from a corrupt read.
        forwarder_key: ForwarderKey::generate(),
        forwarder_endpoint: String::new(),
        peer_node_id: client,
        peers: Vec::new(),
        seq_reservation: 1,
    })?;
    let node_id = node.state.node_id;
    let mut state = node.state.clone();
    // Replace this client's entry rather than appending, so a client that
    // re-bootstraps gets one key rather than accumulating stale ones.
    state.peers.retain(|peer| peer.node_id != client);
    state.peers.push(EnrolledPeer {
        node_id: client,
        pair_key: pair_key.clone(),
    });
    state.peer_node_id = client;
    state.pair_key = pair_key.clone();
    keys::write_node_state(dir, &state)?;
    Ok((node_id, pair_key))
}

/// The address a daemon should bind, given an optional operator-chosen port.
///
/// `0` lets the OS choose, which is the ordinary case: the port is reported back
/// on the connect line, so nothing depends on it being predictable. A fixed port
/// exists for the case where a firewall rule has to name one in advance.
pub fn bind_address(port: u16) -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], port))
}
