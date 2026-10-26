//! Unit tests for `net`.

use super::*;

#[test]
fn the_title_address_prefers_ipv4_and_drops_the_prefix() {
    let interface = NetworkSnapshot {
        interface: "en0".into(),
        up: true,
        connected: true,
        ip_addresses: vec![
            "fe80::1c2b:3cff:fe4d:5e6f/64".into(),
            "192.168.8.66/24".into(),
        ],
        received_bytes_per_second: 0.0,
        transmitted_bytes_per_second: 0.0,
        total_received_bytes: 0,
        total_transmitted_bytes: 0,
    };
    assert_eq!(display_address(&interface), "192.168.8.66");
    let v6_only = NetworkSnapshot {
        ip_addresses: vec!["2001:db8::1/64".into()],
        ..interface.clone()
    };
    assert_eq!(display_address(&v6_only), "2001:db8::1");
    let none = NetworkSnapshot {
        ip_addresses: Vec::new(),
        ..interface
    };
    assert_eq!(display_address(&none), "no IP");
}
