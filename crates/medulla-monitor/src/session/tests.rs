//! Host sampling tests for process-tree readings.

use std::collections::HashMap;

use super::{SessionMonitor, SessionSnapshot};

fn sample() -> SessionSnapshot {
    SessionSnapshot {
        cpu_fraction: 0.25,
        memory_bytes: 1536 * 1024 * 1024,
        total_memory_bytes: 32 * 1024 * 1024 * 1024,
        disk_bytes_per_second: 4096.0,
        processes: 3,
    }
}

#[test]
fn injected_readings_are_returned_for_their_pid_only() {
    let mut monitor = SessionMonitor::default();
    monitor.inject(HashMap::from([(42, sample())]));
    assert_eq!(monitor.sample(42), Some(sample()));
    assert_eq!(monitor.sample(43), None);
}

#[test]
fn a_dead_pid_has_no_reading() {
    let mut monitor = SessionMonitor::default();
    assert_eq!(monitor.sample(u32::MAX), None);
}

#[test]
fn a_live_process_reports_a_bounded_reading() {
    let mut monitor = SessionMonitor::default();
    let pid = std::process::id();
    let sample = monitor.sample(pid).expect("this process is running");
    assert!(sample.processes >= 1, "the root itself must be counted");
    assert!((0.0..=1.0).contains(&sample.cpu_fraction));
    assert!(sample.memory_bytes > 0, "a live process holds memory");
    assert_eq!(sample.disk_bytes_per_second, 0.0);
}

#[test]
fn repeated_samples_are_stable_within_the_throttle_window() {
    let mut monitor = SessionMonitor::default();
    let pid = std::process::id();
    let first = monitor.sample(pid).expect("this process is running");
    assert_eq!(monitor.sample(pid), Some(first));
}
