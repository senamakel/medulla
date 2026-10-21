//! Application callbacks at the edge of a PTY session's lifetime.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Portable facts about a harness session passed to application callbacks.
#[derive(Debug, Clone, Default)]
pub struct LifecycleSession {
    /// Local PTY manager id.
    pub id: String,
    /// External harness session id, when the provider exposes one.
    pub session_id: Option<String>,
    /// Working directory used to locate provider session metadata.
    pub cwd: String,
    /// Environment captured at launch.
    pub env: HashMap<String, String>,
    /// Application-owned capability key to revoke on close.
    pub grant_session: Option<String>,
}

/// Application-defined launch origin carried without importing app enums.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpaqueOrigin(String);

impl OpaqueOrigin {
    /// Store an application-defined origin key.
    pub fn new(value: impl Into<String>) -> Self { Self(value.into()) }

    /// Read the application-defined origin key.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<medulla_types::SessionOrigin> for OpaqueOrigin {
    fn from(origin: medulla_types::SessionOrigin) -> Self { Self::new(origin.as_str()) }
}

/// Application-owned behavior that cannot live in the portable PTY crate.
///
/// The TUI uses these callbacks to revoke session grants and resolve provider
/// specific thread labels while the PTY manager remains independent of the SDK.
pub trait SessionLifecycle: Send + Sync + 'static {
    /// Release application resources associated with a harness session.
    fn on_close(&self, session: &LifecycleSession);

    /// Return a provider-specific thread label for a harness session, if known.
    fn thread_label(&self, session: &LifecycleSession) -> Option<String>;

    /// Inspect a session's current checkout. The TUI owns the Git inspection
    /// policy; the manager only stores the resulting value.
    fn checkout(&self, _session: &LifecycleSession) -> medulla_types::Checkout {
        medulla_types::Checkout::default()
    }
}

/// A once-only notification for one external harness session.
///
/// Reaping may race across the reader and explicit-close paths. The callback
/// can revoke capabilities or clean persisted state, so it must run once.
pub struct SessionClose {
    lifecycle: Arc<dyn SessionLifecycle>,
    session: LifecycleSession,
    notified: AtomicBool,
}

impl SessionClose {
    /// Build a once-only close notification.
    pub fn new(lifecycle: Arc<dyn SessionLifecycle>, session: LifecycleSession) -> Self {
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
