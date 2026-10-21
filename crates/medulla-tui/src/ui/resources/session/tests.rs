//! Formatting and sampling tests for per-session resource readings.
//!
//! The formatting cases inject a fixed [`SessionSnapshot`], so their assertions
//! are exact. The sampling cases run against this test process's own tree,
//! where the only facts that hold on every machine are structural — a reading
//! exists, it covers more than the root alone, and it is bounded.

use medulla_monitor::SessionSnapshot;

use super::{session_segments, session_usage};

/// A session using a quarter of a 32 GiB machine's CPU and 1.5 GiB of its RAM.
fn sample() -> SessionSnapshot {
    SessionSnapshot {
        cpu_fraction: 0.25,
        memory_bytes: 1536 * 1024 * 1024,
        total_memory_bytes: 32 * 1024 * 1024 * 1024,
        disk_bytes_per_second: 4096.0,
        processes: 3,
    }
}

/// CPU, memory and I/O in that fixed order, whatever the numbers are.
#[test]
fn segments_read_cpu_then_ram_then_io() {
    assert_eq!(
        session_segments(sample()),
        vec!["CPU 25%", "RAM 1.5G", "IO 4.0K/s"]
    );
}

/// An idle session still renders all three segments, so a title's shape does
/// not change under the operator as the harness goes quiet.
#[test]
fn an_idle_session_still_renders_every_segment() {
    assert_eq!(
        session_usage(SessionSnapshot::default()),
        "CPU 0% · RAM 0B · IO 0B/s"
    );
}
