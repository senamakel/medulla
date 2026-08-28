//! Closing a harness, local or remote, and reconciling its UI focus state.

use super::super::remote_sessions::SessionRef;
use super::super::types::App;

impl App {
    /// Ask before closing the harness the pane is showing.
    ///
    /// A question rather than a kill, for the same reason the task-kill chord
    /// asks: the child is usually mid-turn, and what it loses is not recoverable
    /// by pressing the key again. Refusing with a reason beats doing nothing —
    /// an operator who pressed `k` and saw nothing cannot tell "wrong row" from
    /// "broken".
    ///
    /// Routed by [`SessionRef`] like every other session action: a remote id
    /// (`host/w_1`) has no orchestrator control to check — a remote session is
    /// always [`SessionControl::User`](crate::worker::pty::SessionControl::User),
    /// see [`session_row`](super::super::remote_sessions::session_row) — so only
    /// whether it is still running is worth asking about.
    pub(crate) fn close_pane_session_prompt(&mut self) {
        let Some(session) = self.pane_session.clone() else {
            self.set_status("No session on this row — select one to close it");
            return;
        };
        if let SessionRef::Local(id) = SessionRef::parse(&session) {
            if self
                .local_sessions
                .as_ref()
                .and_then(|harnesses| harnesses.control(id))
                == Some(crate::worker::pty::SessionControl::Orchestrator)
            {
                self.set_status("Task sessions are view-only while they are running");
                return;
            }
        }
        let running = self
            .session_row(&session)
            .is_some_and(|row| matches!(row.state, crate::worker::pty::PtyState::Running));
        if !running {
            self.set_status("That session has already exited");
            return;
        }
        self.arm_harness_close(session);
    }

    /// Close a harness: kill the child, and tidy up what held it.
    ///
    /// Killing also releases the attachment, returning the keyboard to the
    /// chrome because the pane it was in has stopped listening. For a remote
    /// session this sends [`RemoteRequest::Close`](crate::remote::client::RemoteRequest::Close)
    /// to that host's connection; the daemon closing the pty is what actually
    /// ends the child, so [`release_session`](Self::release_session) runs
    /// immediately rather than waiting for the host to confirm — the same as the
    /// local path, which does not wait for the child to finish exiting either.
    pub(crate) fn close_session(&mut self, session: &str) {
        match SessionRef::parse(session) {
            SessionRef::Local(id) => {
                let Some(harnesses) = self.local_sessions.clone() else {
                    self.set_status("This device is not hosting, so it has no sessions");
                    return;
                };
                // Control may have changed while the confirmation was open.
                // Re-check at the destructive boundary so a stale prompt cannot
                // kill active work.
                if harnesses.control(id) == Some(crate::worker::pty::SessionControl::Orchestrator)
                {
                    self.set_status("Task sessions are view-only while they are running");
                    return;
                }
                if !harnesses.sessions.close(id) {
                    self.set_status("That session is gone");
                    return;
                }
            }
            SessionRef::Remote { host, session } => {
                let session = session.to_string();
                let Some(state) = self.remote_hosts.get_mut(host) else {
                    self.set_status("That host is not connected");
                    return;
                };
                if let Err(reason) =
                    state.request(crate::remote::client::RemoteRequest::Close(session))
                {
                    self.set_status(format!("Could not close it: {reason}"));
                    return;
                }
            }
        }
        if self.harness_focus.is_attached_to(session) {
            self.release_session();
        }
        self.set_status("Closed the harness");
    }
}
