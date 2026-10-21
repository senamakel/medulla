//! Application callbacks at the edge of a PTY session's lifetime.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::attention::HarnessAttention;
use medulla_types::HarnessProvider;

/// Portable facts about a harness session passed to application callbacks.
#[derive(Debug, Clone)]
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
    /// Harness provider for TUI-owned screen classifiers.
    pub provider: HarnessProvider,
}

/// TUI-inspected checkout facts captured immediately before process launch.
#[derive(Debug, Clone, Default)]
pub struct LaunchCheckout {
    /// Checkout shown on the initial row.
    pub checkout: medulla_types::Checkout,
    /// Repository root at launch.
    pub root: Option<String>,
    /// Commit at launch.
    pub commit: Option<String>,
    /// Filesystem identity used to detect checkout replacement.
    pub identity: Option<String>,
}

impl Default for LifecycleSession {
    fn default() -> Self {
        Self {
            id: String::new(),
            session_id: None,
            cwd: String::new(),
            env: HashMap::new(),
            grant_session: None,
            provider: HarnessProvider::Shell,
        }
    }
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

    /// Remove application-owned records when a session is forgotten.
    fn on_forget(&self, _session: &str) {}

    /// Whether the current screen shows a harness still doing visible work.
    fn is_working(&self, _screen: &str) -> bool { false }

    /// Classify a screen into a UI cue. The portable manager owns polling and
    /// cue lifetime; the TUI owns provider-specific recognition.
    fn classify_attention(
        &self,
        _session: &LifecycleSession,
        _screen: &str,
        _now: i64,
    ) -> Option<HarnessAttention> { None }

    /// Inspect a session's current checkout. The TUI owns the Git inspection
    /// policy; the manager only stores the resulting value.
    fn checkout(&self, _session: &LifecycleSession) -> medulla_types::Checkout {
        medulla_types::Checkout::default()
    }

    /// Capture the checkout snapshot before a harness can modify it.
    fn launch_checkout(&self, session: &LifecycleSession) -> LaunchCheckout {
        LaunchCheckout {
            checkout: self.checkout(session),
            ..LaunchCheckout::default()
        }
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
