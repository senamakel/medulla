//! Minting a pairing on this device: the identity the TUI will dial from, and
//! the key the host is told.
//!
//! The client side of `medulla daemon <key>`. Everything in the key is decided
//! here — this device's node id, a node id for the host, the pair key, the
//! port — and written into the host's client state directory before the key
//! is shown, so the dial that follows reads the same material the host was
//! given. The host learns nothing until the operator pastes the key; nothing
//! here touches the network.

use std::path::Path;

use medulla_link::keys::{self, EnrolledPeer, ForwarderKey, HostKey, NodeId, NodeState, Role};

/// Mint (or re-mint) the pairing for the host whose client state lives in
/// `state_dir`, listening on `port`.
///
/// This device's node id is kept across re-issues when one is already on
/// disk — the host is told it in the key either way, so a stable id costs
/// nothing and keeps a dial that races the re-pair from being addressed to a
/// stranger. The host's node id and the pair key are always fresh: a
/// re-issued key must not be a way to recover the old one.
///
/// # Errors
///
/// When the directory cannot be created or written, or when a connection to
/// this host is still holding its identity lock — the caller should drop the
/// connection and try again.
pub fn mint_pairing(state_dir: &Path, port: u16) -> Result<HostKey, String> {
    std::fs::create_dir_all(state_dir).map_err(|error| error.to_string())?;
    let _lock = keys::lock_node_dir(state_dir).map_err(|error| match error {
        keys::KeyError::Busy(_) => {
            "this host is still connected; try again in a moment".to_string()
        }
        other => other.to_string(),
    })?;
    let path = keys::node_path(state_dir);
    let client = keys::read_node_state(&path)
        .ok()
        .map(|state| state.node_id)
        .unwrap_or_else(NodeId::generate);
    let key = HostKey::generate(client, port);
    let state = NodeState {
        version: 2,
        node_id: client,
        // The client half of the direction bit (§4.2): this end dials in.
        role: Role::Orchestrator,
        pair_key: key.pair_key.clone(),
        // Never used on the direct path; present because the schema has it.
        forwarder_key: ForwarderKey::generate(),
        forwarder_endpoint: String::new(),
        peer_node_id: key.host,
        peers: vec![EnrolledPeer {
            node_id: key.host,
            pair_key: key.pair_key.clone(),
        }],
        seq_reservation: 1,
    };
    keys::write_node_state(&path, &state).map_err(|error| error.to_string())?;
    Ok(key)
}

/// Forget a host's pairing on this device.
///
/// # Errors
///
/// When the directory exists and cannot be removed.
pub fn forget_pairing(state_dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(state_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// The command the operator runs on the host, for `key`.
pub fn pairing_command(key: &HostKey) -> String {
    format!("medulla daemon {}", key.encode())
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
