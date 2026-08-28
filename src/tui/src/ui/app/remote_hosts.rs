//! What the client knows about each `[[remoteHosts]]` machine.
//!
//! Deliberately a *cache*, not a connection. The connection itself lives on the
//! event loop's side, because bootstrapping one means running `ssh` and waiting —
//! and the render pass must never wait for anything. What the App holds is the
//! last thing each host said about itself, which is enough to draw a rail lane
//! and fill a picker.
//!
//! # One connection per host, opened when it is needed
//!
//! Not at startup: a configured host that is switched off, or on a network the
//! laptop is not on, must cost nothing until somebody actually asks for it. So a
//! host sits in [`RemoteHostStatus::Idle`] until the operator picks it, and only
//! then is `ssh` run. After that the one link carries every session on that
//! machine — the expensive part is the bootstrap, and it happens once per host
//! rather than once per shell.

use medulla::protocol::{RemoteCapabilities, RemoteSessionRow};

/// Where a remote host is in its connection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::ui::app) enum RemoteHostStatus {
    /// Configured and never dialled. The ordinary state, and free.
    #[default]
    Idle,
    /// `ssh` is running, or the link has not answered yet.
    Connecting,
    /// Answered, and its capabilities are known.
    Live,
    /// The bootstrap failed. Terminal until the operator asks again, and it
    /// carries the reason because "could not connect" on its own is never enough
    /// to act on — the useful cases are an unknown host key, a missing binary
    /// and a refused login, and each has a different fix.
    Failed(String),
}

/// One remote machine as the UI sees it between connections.
#[derive(Debug, Clone, Default)]
pub(in crate::ui::app) struct RemoteHostState {
    /// Where its connection is.
    pub(in crate::ui::app) status: RemoteHostStatus,
    /// What it said it can do. Empty until it answers.
    pub(in crate::ui::app) capabilities: RemoteCapabilities,
    /// Its sessions, as last reported.
    pub(in crate::ui::app) rows: Vec<RemoteSessionRow>,
}

impl RemoteHostState {
    /// Whether this host is worth asking to connect.
    ///
    /// A failed host is, on purpose: the operator picking it again is exactly
    /// how they say "try that once more", and requiring a separate gesture to
    /// clear the failure would be a step with no meaning of its own.
    pub(in crate::ui::app) fn should_dial(&self) -> bool {
        matches!(
            self.status,
            RemoteHostStatus::Idle | RemoteHostStatus::Failed(_)
        )
    }

    /// A short phrase for the rail and the picker.
    pub(in crate::ui::app) fn summary(&self) -> String {
        match &self.status {
            RemoteHostStatus::Idle => "not connected".to_string(),
            RemoteHostStatus::Connecting => "connecting…".to_string(),
            RemoteHostStatus::Live => match self.rows.len() {
                0 => "connected".to_string(),
                1 => "1 session".to_string(),
                n => format!("{n} sessions"),
            },
            RemoteHostStatus::Failed(reason) => reason.clone(),
        }
    }
}
