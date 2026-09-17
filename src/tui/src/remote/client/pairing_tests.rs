//! Tests for minting a pairing: what is kept across re-issues, what is fresh,
//! and that the dial reads back what the host was told.

use std::time::Duration;

use medulla_link::keys;

use super::{forget_pairing, mint_pairing, pairing_command};

#[test]
fn a_pairing_writes_a_dialable_identity_and_a_pasteable_command() {
    let dir = tempfile::tempdir().unwrap();
    let key = mint_pairing(dir.path(), 60001).unwrap();
    assert_eq!(key.port, 60001);

    let state = keys::read_node_state(&keys::node_path(dir.path())).unwrap();
    assert_eq!(state.version, 2);
    assert_eq!(state.node_id, key.client);
    let peers = state.enrolled_peers();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].node_id, key.host);
    assert_eq!(peers[0].pair_key, key.pair_key);

    let command = pairing_command(&key);
    assert!(command.starts_with("medulla daemon HK1-"), "{command}");
}

#[test]
fn re_issuing_keeps_this_device_id_and_replaces_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let first = mint_pairing(dir.path(), 60001).unwrap();
    let second = mint_pairing(dir.path(), 60002).unwrap();
    assert_eq!(first.client, second.client, "this device keeps its id");
    assert_ne!(first.host, second.host, "the host gets a new id");
    assert_ne!(first.pair_key, second.pair_key, "and a new key");
    let state = keys::read_node_state(&keys::node_path(dir.path())).unwrap();
    assert_eq!(state.enrolled_peers()[0].node_id, second.host);
}

#[test]
fn a_held_identity_refuses_a_re_issue_and_forgetting_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    mint_pairing(dir.path(), 60001).unwrap();
    let held = keys::lock_node_dir(dir.path()).unwrap();
    let error = mint_pairing(dir.path(), 60001).unwrap_err();
    assert!(error.contains("still connected"), "{error}");
    drop(held);

    forget_pairing(dir.path()).unwrap();
    assert!(!dir.path().exists());
    forget_pairing(dir.path()).unwrap();
}

#[test]
fn a_lock_released_shortly_after_a_disconnect_still_lets_the_reissue_through() {
    // Disconnecting a host only drops the App's sender; the connection task's
    // identity lock clears on its own next tick, a handful of milliseconds
    // later. `mint_pairing` retries across a short budget for exactly this
    // race, rather than making the operator press `i` twice.
    let dir = tempfile::tempdir().unwrap();
    mint_pairing(dir.path(), 60001).unwrap();
    let held = keys::lock_node_dir(dir.path()).unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        drop(held);
    });
    let key = mint_pairing(dir.path(), 60002).unwrap();
    assert_eq!(key.port, 60002);
}
