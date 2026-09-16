//! Tests for recording pairings: the same key twice, a replaced key, and a
//! port another client already holds on this host.

use medulla_link::keys::{HostKey, NodeId};

use crate::remote::serve::paired::{pair, pairings_on_disk};

#[test]
fn the_same_key_twice_is_one_pairing_and_a_new_key_replaces_it() {
    let home = tempfile::tempdir().unwrap();
    let client = NodeId::generate();
    let first = HostKey::generate(client, 62001);
    pair(home.path(), &first).unwrap();
    pair(home.path(), &first).unwrap();
    assert_eq!(pairings_on_disk(home.path()).len(), 1);

    let second = HostKey::generate(client, 62005);
    pair(home.path(), &second).unwrap();
    let pairings = pairings_on_disk(home.path());
    assert_eq!(pairings.len(), 1, "same client, one pairing");
    assert_eq!(pairings[0].port, 62005);
}

#[test]
fn a_second_client_cannot_take_a_port_already_paired_here() {
    let home = tempfile::tempdir().unwrap();
    pair(home.path(), &HostKey::generate(NodeId::generate(), 62001)).unwrap();
    let error = pair(home.path(), &HostKey::generate(NodeId::generate(), 62001)).unwrap_err();
    assert!(error.to_string().contains("62001"), "{error}");
    assert_eq!(pairings_on_disk(home.path()).len(), 1);
    // A different port is fine: two clients, two pairings.
    pair(home.path(), &HostKey::generate(NodeId::generate(), 62002)).unwrap();
    assert_eq!(pairings_on_disk(home.path()).len(), 2);
}
