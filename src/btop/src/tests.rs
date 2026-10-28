//! btop runs once per process, so its lifecycle is exercised by one test that
//! walks it end to end; the pure helpers get their own.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;

/// Drain until `pred` holds over everything drawn so far, or give up.
#[cfg(not(medulla_btop_unsupported))]
fn drain_until(btop: &Btop, seen: &mut String, pred: impl Fn(&str) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        btop.drain(|bytes| seen.push_str(&String::from_utf8_lossy(bytes)));
        if pred(seen) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[cfg(not(medulla_btop_unsupported))]
#[test]
fn btop_runs_embedded_and_hands_quit_back_to_the_host() {
    let dir = std::env::temp_dir().join(format!("medulla-btop-test-{}", std::process::id()));
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let btop = Btop::start(&dir, 160, 48, move || {
        counter.fetch_add(1, Ordering::Relaxed);
    })
    .expect("btop starts");

    // It draws its boxes into the virtual terminal, and wakes the host to say so.
    let mut seen = String::new();
    assert!(
        drain_until(&btop, &mut seen, |out| out.contains("cpu") && out.contains("mem")),
        "no boxes drawn: {seen:?}"
    );
    assert!(wakes.load(Ordering::Relaxed) > 0);
    assert!(dir.join("btop.log").exists() || dir.exists());

    // Agents are accepted and the agents box is titled for them.
    btop.set_agents(&[AgentTag {
        pid: std::process::id(),
        label: "claude".into(),
        launched: true,
    }]);
    btop.resize(170, 50);
    seen.clear();
    assert!(
        drain_until(&btop, &mut seen, |out| out.contains("agents")),
        "agents box not drawn: {seen:?}"
    );

    // `q` asks the host to leave rather than ending the process.
    assert!(!btop.take_leave());
    btop.send(b"q");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut left = false;
    while !left && Instant::now() < deadline {
        left = btop.take_leave();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(left, "q did not request leave");
    assert_eq!(btop.error(), None);

    // A second start in the same process is refused, not undefined.
    assert_eq!(
        Btop::start(&dir, 80, 24, || {}).unwrap_err(),
        StartError::AlreadyStarted
    );

    btop.set_visible(false);
    btop.set_visible(true);
    btop.stop(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(medulla_btop_unsupported)]
#[test]
fn unsupported_targets_refuse_to_start() {
    assert!(!SUPPORTED);
    assert_eq!(
        Btop::start(std::path::Path::new("."), 80, 24, || {}).unwrap_err(),
        StartError::Unsupported
    );
}

#[test]
fn start_errors_read_as_sentences() {
    assert_eq!(
        StartError::AlreadyStarted.to_string(),
        "btop is already running in this process"
    );
    assert_eq!(
        StartError::Unsupported.to_string(),
        "btop is not available on this platform"
    );
}
