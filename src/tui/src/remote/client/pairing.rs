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
use std::time::Duration;

use medulla_link::keys::{self, EnrolledPeer, ForwarderKey, HostKey, KeyError, NodeId, NodeState, Role};

/// How long re-issuing a key waits for a just-dropped connection's identity
/// lock to clear, and how often it checks.
///
/// Disconnecting a host only drops the App's sender; the connection task
/// notices on its next tick and releases the lock when it unwinds, so the
/// very next `mint_pairing` call — issuing a fresh key for a host that was
/// connected a moment ago — otherwise loses this race almost every time. Both
/// numbers are small because the lock is local-filesystem-only and the task
/// unwind that releases it has nothing to wait on itself.
const LOCK_RETRY_BUDGET: Duration = Duration::from_millis(200);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(10);

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
/// this host is still holding its identity lock after [`LOCK_RETRY_BUDGET`]
/// — the caller should drop the connection and try again.
pub fn mint_pairing(state_dir: &Path, port: u16) -> Result<HostKey, String> {
    std::fs::create_dir_all(state_dir).map_err(|error| error.to_string())?;
    let _lock = lock_node_dir_with_retry(state_dir)?;
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

/// [`keys::lock_node_dir`], retried across [`LOCK_RETRY_BUDGET`] while the
/// error is [`KeyError::Busy`].
///
/// A blocking retry here, rather than in the caller, because the lock is a
/// local file: the wait is bounded and short, and every caller wants the
/// same behaviour, so there is nothing an async retry loop up the stack would
/// buy beyond complexity.
fn lock_node_dir_with_retry(state_dir: &Path) -> Result<keys::NodeLock, String> {
    let deadline = std::time::Instant::now() + LOCK_RETRY_BUDGET;
    loop {
        match keys::lock_node_dir(state_dir) {
            Ok(lock) => return Ok(lock),
            Err(KeyError::Busy(_)) if std::time::Instant::now() < deadline => {
                std::thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(KeyError::Busy(_)) => {
                return Err("this host is still connected; try again in a moment".to_string())
            }
            Err(other) => return Err(other.to_string()),
        }
    }
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
