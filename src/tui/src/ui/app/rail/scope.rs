//! Scoping the rail to the current host.
//!
//! The rail is assembled from a `Host → Agent → Session` tree and a flat list
//! of live sessions. Both are filtered here by [`HostSelection`], so a rail
//! scoped to one machine simply never sees the others' rows, and the cursor,
//! the pane and every action downstream work on fewer rows rather than on a
//! different structure.
//!
//! `All hosts` is the combined view. Sessions that belong to no declared agent
//! are, on this device, listed loose below the tree; with several hosts in
//! view that would leave a remote shell with no sign of *where* it runs. So in
//! that view each host's loose sessions are folded into a group of their own
//! under their host, and the existing host headers — shown whenever the tree
//! has more than one host — label them.

use medulla::ui::hosts::HostRow;

use super::super::hosts::HostSelection;
use super::super::remote_sessions::SessionRef;
use super::super::types::App;
use super::types::{AgentGroup, AgentRailRow, HostGroup, SessionRailRow};
use crate::worker::pty::SessionRow;

impl App {
    /// The host a session row runs on: its `[[remoteHosts]]` id, or empty for
    /// this device.
    pub(in crate::ui::app) fn session_host_id(row: &SessionRow) -> &str {
        match SessionRef::parse(&row.id) {
            SessionRef::Remote { host, .. } => host,
            SessionRef::Local(_) => "",
        }
    }

    /// Whether a session row is in the current scope.
    pub(in crate::ui::app) fn session_in_scope(&self, row: &SessionRow) -> bool {
        self.current_host.includes(Self::session_host_id(row))
    }

    /// Keep only the hosts the current scope shows.
    pub(super) fn scope_host_tree(&self, rows: Vec<HostRow>) -> Vec<HostRow> {
        rows.into_iter()
            .filter(|row| match &self.current_host {
                HostSelection::All => true,
                HostSelection::ThisDevice => row.kind != medulla::ui::hosts::HostKind::Remote,
                HostSelection::Host(id) => {
                    row.kind == medulla::ui::hosts::HostKind::Remote && &row.id == id
                }
            })
            .collect()
    }

    /// In the combined view, file loose sessions under their host so the host
    /// header labels them.
    ///
    /// Only under the default host grouping: the path and harness groupings
    /// section by properties a loose session's synthetic agent does not have,
    /// and would file every one of them under "no path".
    pub(super) fn fold_orphans_by_host(
        &self,
        hosts: &mut [HostGroup],
        orphans: Vec<SessionRailRow>,
    ) -> Vec<SessionRailRow> {
        let combined = self.current_host == HostSelection::All
            && hosts.len() > 1
            && self.loaded.config.appearance.sidebar_grouping
                == medulla::config::SidebarGrouping::Host;
        if !combined {
            return orphans;
        }
        let local_host = self.local_host_id();
        let mut leftover = Vec::new();
        for session in orphans {
            let host_id = session
                .local
                .as_ref()
                .map(|row| Self::session_host_id(row).to_string())
                .unwrap_or_default();
            let wanted = if host_id.is_empty() {
                local_host.clone()
            } else {
                host_id
            };
            let Some(host) = hosts.iter_mut().find(|host| host.row.host_id == wanted) else {
                leftover.push(session);
                continue;
            };
            let group_id = format!("sessions@{wanted}");
            let group = match host
                .agents
                .iter_mut()
                .position(|group| group.row.agent_id == group_id)
            {
                Some(index) => &mut host.agents[index],
                None => {
                    host.agents.push(AgentGroup {
                        row: AgentRailRow {
                            agent_id: group_id,
                            host_id: wanted,
                            agent: None,
                            lane_index: None,
                        },
                        sessions: Vec::new(),
                        last_at: 0,
                        lane_label: None,
                        harness_label: None,
                        visible_tasks: 0,
                        hidden: 0,
                        overflow: false,
                    });
                    host.agents.last_mut().expect("just pushed")
                }
            };
            if let Some(row) = &session.local {
                group.last_at = group.last_at.max(row.started_at);
            }
            group.sessions.push(session);
        }
        leftover
    }
}
