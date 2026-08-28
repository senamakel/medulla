//! Remote sessions as the rail and the pane already understand them.
//!
//! # Why a synthesized `SessionRow` rather than a second row type
//!
//! The rail, the pane, the close confirmation, the attention badge and the
//! keyboard all speak [`SessionRow`]. Adding a parallel remote type would mean
//! teaching every one of them a second vocabulary, and the two would drift —
//! which is the whole failure this avoids. So a remote session is translated
//! *into* a `SessionRow` once, here, and everything downstream is unchanged.
//!
//! The fields a remote session genuinely does not have are left honestly empty
//! rather than guessed at. Checkout tracking, the launch commit and the MCP grant
//! key all describe things on *this* filesystem; inventing values for them would
//! make the Changes view claim to know about a repository it cannot see.
//!
//! # Ids
//!
//! A local id stays bare (`w_7`), so every existing path, status message and test
//! is byte-for-byte unchanged. A remote one is `<hostId>/w_7`. The separator is
//! the whole resolution rule — see [`SessionRef`].

use medulla::protocol::{RemoteSessionRow, RemoteSessionState};

use super::types::App;
use crate::worker::pty::{PtyState, SessionControl, SessionOrigin, SessionRow};

/// The character separating a host id from a session id.
///
/// A path separator on purpose: it cannot occur in a `w_…` id, and it reads as
/// "there, then this" — which is exactly what a remote session id means.
pub const HOST_SEPARATOR: char = '/';

/// A session id, and where it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRef<'a> {
    /// On this machine. The id is exactly what the pty manager knows it by.
    Local(&'a str),
    /// On another machine.
    Remote {
        /// The `[[remoteHosts]]` id.
        host: &'a str,
        /// The session id as *that host* knows it — never the prefixed form.
        session: &'a str,
    },
}

impl<'a> SessionRef<'a> {
    /// Resolve an id the UI is holding.
    ///
    /// Anything without a separator is local, which is what keeps every existing
    /// caller working untouched.
    pub fn parse(id: &'a str) -> Self {
        match id.split_once(HOST_SEPARATOR) {
            Some((host, session)) if !host.is_empty() && !session.is_empty() => {
                SessionRef::Remote { host, session }
            }
            _ => SessionRef::Local(id),
        }
    }

    /// The host id, when this names a remote session.
    pub fn host(&self) -> Option<&'a str> {
        match self {
            SessionRef::Local(_) => None,
            SessionRef::Remote { host, .. } => Some(host),
        }
    }
}

/// The prefixed id a remote session is filed under on this side.
pub fn qualify(host_id: &str, session_id: &str) -> String {
    format!("{host_id}{HOST_SEPARATOR}{session_id}")
}

/// Translate one remote session into the row every surface already reads.
pub fn session_row(host_id: &str, remote: &RemoteSessionRow) -> SessionRow {
    SessionRow {
        id: qualify(host_id, &remote.id),
        label: remote.label.clone(),
        provider: medulla::protocol::HarnessProvider::from_wire(&remote.provider)
            .unwrap_or(medulla::protocol::HarnessProvider::Shell),
        preset: remote.preset.clone(),
        state: match remote.state {
            RemoteSessionState::Running => PtyState::Running,
            RemoteSessionState::Exited { code } => PtyState::Exited { code },
            RemoteSessionState::Failed => PtyState::Failed,
        },
        cwd: remote.cwd.clone(),
        // Everything below describes this machine's filesystem, and a remote
        // session is not on it. Left empty rather than invented: the Changes
        // view reads these, and a fabricated checkout would have it claim to
        // know a repository it cannot see.
        checkout: medulla::ui::checkout::Checkout::default(),
        launch_root: None,
        launch_commit: None,
        launch_checkout_identity: None,
        session_id: None,
        thread_name: remote.thread_name.clone(),
        started_at: remote.started_at,
        last_output_at: remote.last_output_at,
        last_error: remote.last_error.clone(),
        busy: remote.busy,
        // Always the operator's. A remote session exists because somebody asked
        // for it in the picker, and nothing dispatches into one — so the
        // orchestrator must never see it as available.
        control: SessionControl::User,
        origin: SessionOrigin::User,
        retained: false,
        closed_by_request: false,
        name: remote.name.clone(),
        attention: remote.attention.as_ref().map(|attention| {
            crate::worker::pty::HarnessAttention {
                // An unrecognized kind becomes `Choice` — the "it is asking, and
                // we cannot name what for" cue. Dropping the whole cue instead
                // would hide a harness that is genuinely stuck, which is the one
                // thing the rail exists to surface.
                kind: crate::worker::pty::AttentionKind::from_str(&attention.kind)
                    .unwrap_or(crate::worker::pty::AttentionKind::Choice),
                what: attention.summary.clone(),
                // The sender's `since` is a reading from its clock, so it never
                // crosses. This is the honest local answer: the rail uses it only
                // to say how long the cue has held, and we have known since we
                // last heard from the session.
                since: remote.last_output_at,
            }
        }),
        working: remote.working,
        mcp_grant_session: None,
    }
}

impl App {
    /// Every remote session, as rows the rail can place.
    pub(in crate::ui::app) fn remote_session_rows(&self) -> Vec<SessionRow> {
        self.remote_hosts
            .iter()
            .flat_map(|(host_id, state)| {
                state
                    .rows
                    .iter()
                    .map(move |remote| session_row(host_id, remote))
            })
            .collect()
    }
}
