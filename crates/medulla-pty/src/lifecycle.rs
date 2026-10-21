//! Application callbacks at the edge of a PTY session's lifetime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Application-owned behavior that cannot live in the portable PTY crate.
///
/// The TUI uses these callbacks to revoke session grants and resolve provider
/// specific thread labels while the PTY manager remains independent of the SDK.
pub trait SessionLifecycle: Send + Sync + 'static {
    /// Release application resources associated with a harness session.
    fn on_close(&self, session: &str);

    /// Return a provider-specific thread label for a harness session, if known.
    fn thread_label(&self, session: &str) -> Option<String>;
}

/// A once-only notification for one external harness session.
///
/// Reaping may race across the reader and explicit-close paths. The callback
/// can revoke capabilities or clean persisted state, so it must run once.
pub struct SessionClose {
    lifecycle: Arc<dyn SessionLifecycle>,
    session: String,
    notified: AtomicBool,
}

impl SessionClose {
    /// Build a once-only close notification.
    pub fn new(lifecycle: Arc<dyn SessionLifecycle>, session: String) -> Self {
        Self {
            lifecycle,
            session,
            notified: AtomicBool::new(false),
        }
    }

    /// Notify the application once, even if multiple paths reap the session.
    pub fn notify(&self) {
        if !self.notified.swap(true, Ordering::AcqRel) {
            self.lifecycle.on_close(&self.session);
        }
    }
}
