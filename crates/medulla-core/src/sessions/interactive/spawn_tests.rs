//! Spawn resilience for the interactive transport.
//!
//! Linux refuses to `exec` a file while any process holds it open for writing
//! (ETXTBSY, "Text file busy"). A concurrently forked child inherits such a
//! handle until its own `exec`, so a freshly written or freshly replaced
//! harness binary can be briefly busy. The headless executor and the PTY
//! manager already retry through that window; these tests pin the same for
//! [`InteractiveSession::open`].

use super::session::InteractiveSession;
use super::tests::fake_harness;

#[tokio::test]
async fn open_waits_out_a_briefly_busy_executable() {
    let dir = tempfile::tempdir().unwrap();
    let spec = fake_harness(dir.path(), "cat >/dev/null");
    // Stand in for the inherited handle: hold the script open for writing and
    // release it shortly after the spawn has first been refused.
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&spec.bin)
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(writer);
    });

    let session = InteractiveSession::open(&spec)
        .await
        .expect("a momentarily busy executable must still open");

    release.join().unwrap();
    session.close().await;
}
