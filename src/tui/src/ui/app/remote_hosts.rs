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

use super::types::App;
use crate::ui::harness_pane::HarnessChoice;

impl App {
    /// What `host_id` said it can start, translated into picker rows.
    ///
    /// Empty when the host has not answered yet, which the picker reads as "ask
    /// it" rather than "it offers nothing" — the two are different, and only the
    /// second is worth telling the operator about.
    ///
    /// A remote host's list comes from the host and never from this machine's
    /// own: offering a `claude` row for a box that has none would break the
    /// picker's standing promise that every row it shows can actually start.
    pub(in crate::ui::app) fn remote_choices(&self, host_id: &str) -> Vec<HarnessChoice> {
        let Some(state) = self.remote_hosts.get(host_id) else {
            return Vec::new();
        };
        state
            .capabilities
            .harnesses
            .iter()
            .filter_map(|choice| {
                medulla::protocol::HarnessProvider::from_wire(&choice.provider)
                    .map(HarnessChoice::native)
            })
            .collect()
    }

    /// The cached state for `host_id`, if it has any.
    pub(in crate::ui::app) fn remote_host(&self, host_id: &str) -> Option<&RemoteHostState> {
        self.remote_hosts.get(host_id)
    }

    /// Record that a host is being dialled.
    pub(in crate::ui::app) fn remote_host_connecting(&mut self, host_id: &str) {
        self.remote_hosts
            .entry(host_id.to_string())
            .or_default()
            .status = RemoteHostStatus::Connecting;
    }

    /// Fold in what a host answered with.
    pub fn remote_host_connected(&mut self, host_id: &str, capabilities: RemoteCapabilities) {
        let state = self.remote_hosts.entry(host_id.to_string()).or_default();
        state.status = RemoteHostStatus::Live;
        state.capabilities = capabilities;
    }

    /// Record that a host could not be reached, and why.
    pub fn remote_host_failed(&mut self, host_id: &str, reason: String) {
        self.remote_hosts
            .entry(host_id.to_string())
            .or_default()
            .status = RemoteHostStatus::Failed(reason);
    }

    /// Replace a host's session list.
    pub fn remote_host_sessions(&mut self, host_id: &str, rows: Vec<RemoteSessionRow>) {
        self.remote_hosts
            .entry(host_id.to_string())
            .or_default()
            .rows = rows;
    }
}

impl App {
    /// Leave `cmd` for [`on_event`](App::on_event) to collect.
    ///
    /// For handlers with no way to return one — see
    /// [`pending_cmds`](crate::ui::app::types::App::pending_cmds).
    pub(in crate::ui::app) fn queue_cmd(&mut self, cmd: crate::ui::app::types::Cmd) {
        self.pending_cmds.push_back(cmd);
    }

    /// Take the next queued command, if any.
    pub(in crate::ui::app) fn take_queued_cmd(&mut self) -> Option<crate::ui::app::types::Cmd> {
        self.pending_cmds.pop_front()
    }
}
