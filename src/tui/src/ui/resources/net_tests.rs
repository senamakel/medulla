//! Unit tests for `net`.

use super::*;

#[test]
fn scaler_tracks_steady_burst_idle_with_five_sample_windows() {
    let mut scale = NetScaler::new(10.0 * 1024.0);
    let steady = (0..6).map(|_| scale.sample(2_000.0)).collect::<Vec<_>>();
    assert_eq!(steady, vec![10_240.0; 6]);
    let burst = (0..5).map(|_| scale.sample(20_000.0)).collect::<Vec<_>>();
    assert_eq!(burst[..4], [10_240.0; 4]);
    assert_eq!(burst[4], 26_000.0);
    let idle = (0..5).map(|_| scale.sample(0.0)).collect::<Vec<_>>();
    assert_eq!(idle[..4], [26_000.0; 4]);
    assert_eq!(idle[4], 10_240.0);
}

#[test]
fn scaler_counters_balance_instead_of_resetting_on_neutral_samples() {
    let mut scale = NetScaler::default();
    for rate in [20_000.0, 20_000.0, 20_000.0, 20_000.0, 10_240.0, 20_000.0] {
        scale.sample(rate);
    }
    assert_eq!(scale.max(), 23_462.0);
}

#[test]
fn interface_cycling_skips_down_interfaces() {
    let interfaces = [("lo", true), ("eth0", false), ("wlan0", true)];
    assert_eq!(
        cycle_interface(Some("lo"), &interfaces, true).as_deref(),
        Some("wlan0")
    );
    assert_eq!(
        cycle_interface(Some("lo"), &interfaces, false).as_deref(),
        Some("wlan0")
    );
}

#[test]
fn default_interface_picks_first_up_non_loopback_with_traffic() {
    let samples = vec![
        crate::ui::resources::sampler::NetworkSnapshot {
            interface: "lo".into(),
            up: true,
            connected: true,
            ip_addresses: vec![],
            received_bytes_per_second: 4.0,
            transmitted_bytes_per_second: 0.0,
            total_received_bytes: 4,
            total_transmitted_bytes: 0,
        },
        crate::ui::resources::sampler::NetworkSnapshot {
            interface: "eth0".into(),
            up: false,
            connected: false,
            ip_addresses: vec![],
            received_bytes_per_second: 0.0,
            transmitted_bytes_per_second: 0.0,
            total_received_bytes: 1_000_000,
            total_transmitted_bytes: 1_000_000,
        },
        crate::ui::resources::sampler::NetworkSnapshot {
            interface: "wlan0".into(),
            up: true,
            connected: true,
            ip_addresses: vec![],
            received_bytes_per_second: 4.0,
            transmitted_bytes_per_second: 0.0,
            total_received_bytes: 4,
            total_transmitted_bytes: 0,
        },
        crate::ui::resources::sampler::NetworkSnapshot {
            interface: "enp2s0".into(),
            up: true,
            connected: true,
            ip_addresses: vec![],
            received_bytes_per_second: 8.0,
            transmitted_bytes_per_second: 0.0,
            total_received_bytes: 50,
            total_transmitted_bytes: 0,
        },
    ];
    assert_eq!(default_interface(&samples), Some("wlan0"));
}

#[test]
fn default_interface_prefers_a_routable_address_over_link_local_traffic() {
    let interface =
        |name: &str, addresses: &[&str]| crate::ui::resources::sampler::NetworkSnapshot {
            interface: name.into(),
            up: true,
            connected: true,
            ip_addresses: addresses.iter().map(|address| (*address).into()).collect(),
            received_bytes_per_second: 0.0,
            transmitted_bytes_per_second: 0.0,
            total_received_bytes: 1_000,
            total_transmitted_bytes: 1_000,
        };
    // macOS order: loopback, the AirDrop link with only a link-local
    // address, then the Wi-Fi interface that actually carries traffic.
    let samples = vec![
        interface("lo0", &["127.0.0.1/8", "::1/128"]),
        interface("awdl0", &["fe80::b4ef:e6ff:fed7:6d54/64"]),
        interface("en0", &["fe80::1c2b:3cff:fe4d:5e6f/64", "192.168.8.66/24"]),
    ];
    assert_eq!(default_interface(&samples), Some("en0"));
}

#[test]
fn sync_uses_larger_direction_scale() {
    assert_eq!(synced_max(10.0, 20.0, true), (20.0, 20.0));
}
