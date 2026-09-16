//! Tests for recording pairings: the same key twice, a replaced key, and a
//! port another client already holds on this host — plus that `--host`
//! tolerates one pairing's identity being unreadable rather than refusing to
//! serve any of them.

use std::collections::HashMap;
use std::time::Duration;

use medulla_link::keys::{HostKey, NodeId};

use crate::remote::serve::paired::{pair, pairings_on_disk, run_in};

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_mode_skips_a_pairing_with_no_readable_identity_and_serves_the_rest() {
    let home_root = tempfile::tempdir().unwrap();
    let mut env: HashMap<String, String> = HashMap::new();
    env.insert(
        "MEDULLA_HOME".to_string(),
        home_root.path().to_string_lossy().into_owned(),
    );
    let home = medulla::home::medulla_home(&env);

    // A healthy pairing…
    let good = pair(&home, &HostKey::generate(NodeId::generate(), 62101)).unwrap();
    // …and one whose `link/node.json` is gone by the time `--host` reads it —
    // `pairing.json` survives (the daemon still lists it), but the identity
    // behind it does not.
    let bad = pair(&home, &HostKey::generate(NodeId::generate(), 62102)).unwrap();
    let bad_identity = crate::remote::serve::client_dir(&home, bad.client);
    std::fs::remove_file(medulla_link::keys::node_path(
        &crate::remote::serve::link_dir(&bad_identity),
    ))
    .unwrap();
    assert_eq!(pairings_on_disk(&home).len(), 2, "both still on disk");

    // `run_in` normally serves until killed, so it returning at all within a
    // short window — rather than the test's timeout firing — means it bailed
    // out early instead of skipping the unusable pairing and serving the
    // healthy one.
    let args = vec!["--host".to_string()];
    let outcome = tokio::time::timeout(Duration::from_millis(800), run_in(&env, &args)).await;
    assert!(
        outcome.is_err(),
        "expected run_in to still be serving the healthy pairing, but it returned: {outcome:?}"
    );
    let _ = good;
}
