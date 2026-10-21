//! Host sampling tests for whole-device readings.

use super::DeviceMonitor;
use crate::DeviceSnapshot;

fn sample() -> DeviceSnapshot {
    DeviceSnapshot {
        cpu_fraction: Some(0.25),
        memory_used_bytes: Some(8 * 1024 * 1024 * 1024),
        memory_total_bytes: Some(32 * 1024 * 1024 * 1024),
        disk_used_bytes: Some(300 * 1024 * 1024 * 1024),
        disk_total_bytes: Some(400 * 1024 * 1024 * 1024),
    }
}

#[test]
fn an_injected_reading_is_returned_verbatim() {
    let mut monitor = DeviceMonitor::default();
    monitor.inject(sample());
    assert_eq!(monitor.sample(), sample());
    assert_eq!(monitor.sample(), sample());
}

#[test]
fn a_diskless_sample_can_be_upgraded_within_the_refresh_interval() {
    let mut monitor = DeviceMonitor::default();
    let without_disk = monitor.sample_for(false);
    assert_eq!(without_disk.disk_used_bytes, None);
    assert_eq!(without_disk.disk_total_bytes, None);

    let with_disk = monitor.sample_for(true);
    assert_eq!(
        with_disk.disk_used_bytes.is_some(),
        with_disk.disk_total_bytes.is_some()
    );
}

#[test]
fn live_sampling_is_self_consistent_and_withholds_cpu_on_the_first_reading() {
    let mut monitor = DeviceMonitor::default();
    let first = monitor.sample();
    assert_eq!(first.cpu_fraction, None, "{first:?}");
    assert_eq!(monitor.sample(), first);

    let second = monitor.sample();
    if let Some(fraction) = second.cpu_fraction {
        assert!((0.0..=1.0).contains(&fraction), "{second:?}");
    }
    for (used, total) in [
        (second.memory_used_bytes, second.memory_total_bytes),
        (second.disk_used_bytes, second.disk_total_bytes),
    ] {
        assert_eq!(used.is_some(), total.is_some(), "{second:?}");
        if let Some((used, total)) = used.zip(total) {
            assert!(total > 0 && used <= total, "{second:?}");
        }
    }
}
