//! TUI adapter for the portable PTY session manager.

use std::ops::Deref;
use std::sync::{Arc, OnceLock};

use medulla::harness_hooks::HookEventLog;
use medulla_pty::worker::pty::manager::PtyManager as PortablePtyManager;
use medulla_pty::worker::pty::types::LaunchSpec as PortableLaunchSpec;
use medulla_pty::{HarnessAttention, LaunchCheckout, LifecycleSession, OpaqueOrigin, SessionLifecycle};

use super::types::LaunchSpec;
use super::{attention, launch};

mod clipboard;
pub use medulla_pty::ClipboardSink;

/// Owns the portable manager and adapts TUI launch and lifecycle types.
#[derive(Clone)]
pub struct PtyManager {
    inner: PortablePtyManager,
    lifecycle: Option<Arc<TuiSessionLifecycle>>,
}

impl Deref for PtyManager {
    type Target = PortablePtyManager;
    fn deref(&self) -> &Self::Target { &self.inner }
}

impl Default for PtyManager {
    fn default() -> Self { Self::new() }
}

impl PtyManager {
    /// Build an empty manager with TUI callbacks.
    pub fn new() -> Self { Self::with_now(Arc::new(medulla::clock::now_millis)) }

    /// Override the clock while keeping TUI callbacks.
    pub fn with_now(now: medulla_pty::worker::pty::manager::NowFn) -> Self {
        Self::with_now_and_clipboard(now, clipboard::default_sink())
    }

    /// Override the clipboard destination while keeping TUI callbacks.
    pub fn with_now_and_clipboard(
        now: medulla_pty::worker::pty::manager::NowFn,
        clipboard: medulla_pty::ClipboardSink,
    ) -> Self {
        let lifecycle = Arc::new(TuiSessionLifecycle::default());
        let inner = PortablePtyManager::with_now_clipboard_and_lifecycle(now, clipboard, lifecycle.clone());
        Self { inner, lifecycle: Some(lifecycle) }
    }

    /// Construct a manager with a caller-supplied lifecycle, mainly for tests.
    pub fn with_now_clipboard_and_lifecycle(
        now: medulla_pty::worker::pty::manager::NowFn,
        clipboard: medulla_pty::ClipboardSink,
        lifecycle: Arc<dyn SessionLifecycle>,
    ) -> Self {
        Self {
            inner: PortablePtyManager::with_now_clipboard_and_lifecycle(now, clipboard, lifecycle),
            lifecycle: None,
        }
    }

    /// Supply the hook log used by the TUI attention classifier.
    pub fn set_hook_log(&self, log: HookEventLog) {
        if let Some(lifecycle) = &self.lifecycle {
            let _ = lifecycle.hook_log.set(log);
        }
    }

    /// Resolve provider-specific argv in the TUI, then open a portable session.
    pub fn open(&self, spec: LaunchSpec) -> Result<String, String> {
        let session_id = spec.session_id.or_else(|| launch::mint_session_id(spec.provider));
        let args = launch::interactive_args(
            spec.provider, session_id.as_deref(), spec.skip_permissions,
            spec.model.as_deref(), &spec.extra_args,
        );
        self.inner.open(PortableLaunchSpec {
            provider: spec.provider, preset: spec.preset, bin: spec.bin, cwd: spec.cwd,
            env: spec.env, args, label: spec.label, session_id, control: spec.control,
            origin: OpaqueOrigin::from(spec.origin), name: spec.name,
            mcp_grant_session: spec.mcp_grant_session,
        })
    }
}

#[derive(Default)]
struct TuiSessionLifecycle { hook_log: OnceLock<HookEventLog> }

impl SessionLifecycle for TuiSessionLifecycle {
    fn on_close(&self, session: &LifecycleSession) {
        #[cfg(feature = "workflows")]
        if let Some(grant) = &session.grant_session { medulla::mcp::revoke_session(grant); }
        #[cfg(not(feature = "workflows"))]
        let _ = session;
    }

    fn on_forget(&self, session: &str) {
        #[cfg(feature = "workflows")]
        medulla::mcp::forget_session_runs(session);
        #[cfg(not(feature = "workflows"))]
        let _ = session;
    }

    fn thread_label(&self, session: &LifecycleSession) -> Option<String> {
        match session.session_id.as_deref() {
            Some(id) => medulla::session_history::codex_thread_label(&session.env, id),
            None => medulla::session_history::codex_thread_label_for_cwd(&session.env, &session.cwd),
        }
    }

    fn checkout(&self, session: &LifecycleSession) -> medulla_pty::Checkout {
        super::checkout::inspect::resolve(&session.cwd)
    }

    fn launch_checkout(&self, session: &LifecycleSession) -> LaunchCheckout {
        let cwd = std::path::Path::new(&session.cwd);
        LaunchCheckout {
            checkout: super::checkout::inspect::resolve(&session.cwd),
            root: git_value(cwd, &["rev-parse", "--show-toplevel"]),
            commit: git_value(cwd, &["rev-parse", "--verify", "HEAD"]),
            identity: super::checkout::capture(cwd),
        }
    }

    fn is_working(&self, screen: &str) -> bool { attention::is_working(screen) }

    fn classify_attention(&self, session: &LifecycleSession, screen: &str, now: i64) -> Option<HarnessAttention> {
        let working = attention::is_working(screen);
        let cue = attention::detect(session.provider, screen).or_else(|| {
            self.hook_log.get().and_then(|log| attention::hook_attention(
                session.provider, session.grant_session.as_deref(), working, log,
            ))
        });
        cue.map(|(kind, what)| HarnessAttention::new(kind, what, now))
    }
}

fn git_value(cwd: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    }).filter(|value| !value.is_empty())
}
