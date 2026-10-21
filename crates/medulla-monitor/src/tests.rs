//! Host-independent tests for process sampling behavior.

use super::{disk_rates, ResourceMonitor, ResourceSnapshot};

#[test]
fn first_disk_delta_only_primes_the_counter_baseline() {
    assert_eq!(disk_rates(8_192, 4_096, 1.0, false), (0.0, 0.0));
    assert_eq!(disk_rates(8_192, 4_096, 2.0, true), (4_096.0, 2_048.0));
}

#[test]
fn an_injected_process_reading_bypasses_host_sampling() {
    let expected = ResourceSnapshot {
        cpu_fraction: 0.25,
        memory_bytes: 512,
        total_memory_bytes: 1024,
        disk_read_bytes_per_second: 12.0,
        disk_write_bytes_per_second: 34.0,
        disk_peak_bytes_per_second: 34.0,
    };
    let mut monitor = ResourceMonitor::default();
    monitor.inject(expected);

    assert_eq!(monitor.sample(), expected);
    assert_eq!(monitor.sample(), expected);
}

#[test]
fn live_process_sampling_stays_within_bounds() {
    let mut monitor = ResourceMonitor::default();
    let sample = monitor.sample();
    assert!((0.0..=1.0).contains(&sample.cpu_fraction), "{sample:?}");
    assert!(sample.total_memory_bytes > 0, "{sample:?}");
}
