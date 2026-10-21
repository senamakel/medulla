use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::SessionLifecycle;

struct RecordingLifecycle(Arc<AtomicUsize>);

impl SessionLifecycle for RecordingLifecycle {
    fn on_close(&self, _session: &str) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn thread_label(&self, _session: &str) -> Option<String> {
        None
    }
}

#[test]
fn a_session_close_notifies_its_lifecycle_once() {
    let closes = Arc::new(AtomicUsize::new(0));
    let lifecycle = RecordingLifecycle(Arc::clone(&closes));
    let close = crate::SessionClose::new(Arc::new(lifecycle), "harness-session".to_owned());

    close.notify();
    close.notify();

    assert_eq!(closes.load(Ordering::SeqCst), 1);
}
