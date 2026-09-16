//! One peer to one roster slot — the rule that keeps dispatch honest.

use super::helpers::hw;

#[test]
fn one_peer_never_occupies_two_roster_slots() {
    use super::super::super::roster::remove_conflicting;

    // `MEDULLA_HUB_WORKERS="alpha=<addr>"` seeds the id `alpha`; adding the same
    // address in the TUI uses the address as the id. Same wallet, two names.
    let mut roster = vec![hw("alpha", "So1anaAddr")];
    let incoming = hw("So1anaAddr", "So1anaAddr");
    let removed = remove_conflicting(&mut roster, &incoming);
    roster.push(incoming);

    assert_eq!(removed, vec!["alpha"]);
    assert_eq!(roster.len(), 1, "one destination, one entry");
    assert_eq!(roster[0].id, "So1anaAddr", "the newest naming wins");
}

#[test]
fn re_adding_the_same_id_still_replaces() {
    use super::super::super::roster::remove_conflicting;

    let mut roster = vec![hw("w1", "addr-a")];
    let incoming = hw("w1", "addr-b");
    let removed = remove_conflicting(&mut roster, &incoming);
    roster.push(incoming);

    assert_eq!(removed, vec!["w1"]);
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].address, "addr-b", "an id can be repointed");
}

#[test]
fn distinct_peers_are_left_alone() {
    use super::super::super::roster::remove_conflicting;

    let mut roster = vec![hw("w1", "addr-a"), hw("w2", "addr-b")];
    let incoming = hw("w3", "addr-c");
    let removed = remove_conflicting(&mut roster, &incoming);
    roster.push(incoming);

    assert!(removed.is_empty());
    assert_eq!(roster.len(), 3, "deduping must not collapse real peers");
}

#[test]
fn blank_addresses_do_not_collide_with_each_other() {
    // Two entries with no address are not "the same peer"; collapsing them would
    // silently delete a roster row on an unrelated add.
    use super::super::super::roster::remove_conflicting;

    let mut roster = vec![hw("w1", "")];
    let incoming = hw("w2", "");
    remove_conflicting(&mut roster, &incoming);
    roster.push(incoming);

    assert_eq!(roster.len(), 2);
}

#[test]
fn a_handle_is_recognised_as_an_alias_not_an_address() {
    use super::super::super::handle::is_handle;

    // Contacts, pre-key bundles and DMs are all keyed on the cryptoId; an
    // `@handle` is only a directory alias, and passing it through unresolved
    // produces `POST /contacts/%40name`, which cannot match anything.
    assert!(is_handle("@alice"));
    assert!(is_handle("  @alice"), "leading space is still a handle");
    assert!(
        !is_handle("8m6ZTfUGMdnoWanb1V31SZncBfr9xA1oAXnkv4cAAHVB"),
        "a cryptoId is already the key"
    );
    assert!(!is_handle(""));
}

#[test]
fn an_implausible_address_is_refused_before_it_reaches_the_relay() {
    use super::super::super::handle::is_plausible_address;

    // A stray `>` was accepted as a worker address, registered in the roster,
    // and registered as a worker. Nothing downstream can tell that
    // from a real peer that simply never replies.
    assert!(!is_plausible_address(">"));
    assert!(!is_plausible_address(""));
    assert!(!is_plausible_address("   "));
    assert!(!is_plausible_address("too-short"));
    assert!(
        !is_plausible_address("3Hob1FxUwsy1K2rweppbmCkuPef6unAr5Amj6kQ2fM0A"),
        "base58 excludes 0, O, I and l because they are easy to confuse"
    );

    // Real values must still pass.
    assert!(is_plausible_address(
        "3Hob1FxUwsy1K2rweppbmCkuPef6unAr5Amj6kQ2fM3A"
    ));
    assert!(is_plausible_address(
        "8m6ZTfUGMdnoWanb1V31SZncBfr9xA1oAXnkv4cAAHVB"
    ));
    assert!(is_plausible_address("@alice"));
    assert!(!is_plausible_address("@"), "a bare @ names nobody");
}

