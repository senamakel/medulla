//! The Agents rail: one cursor over the whole `Host → Agent → Session` tree.
//!
//! The rail used to concatenate two lists — the lanes the event fold produced,
//! and the harnesses the operator had started under a `── your harnesses ──`
//! divider — and that split is exactly what the agent/session redefinition
//! removes. A task *is* an agent session; a harness is not an entity at all, only
//! the type an agent runs. So the rail now renders one tree:
//!
//! ```text
//! ◆ orchestrator            ← the conversation (not an agent)
//! + New agent               ← declares one on this machine
//! ▸ this device             ← host row, only when a remote host exists
//!   ● medulla-claude        ← DECLARED agent: present with zero sessions
//!     ├ t_41 · running      ← a session the orchestrator dispatched
//!     └ debug login         ← a session the operator started
//! ```
//!
//! **Agents come from the tree, not from traffic**: a lane is folded from task
//! events, so an agent nothing had been dispatched to produced no row at all —
//! which made the rail a list of what happened rather than of what exists.
//!
//! The host and agent levels are the shared `Host → Agent` projection
//! ([`medulla::ui::hosts::host_rows`]) — literally the same call the Hosts tab
//! renders, so the two lenses cannot disagree about what exists. Lanes attach to
//! the agents it produces; a lane for an agent the projection does not know (a
//! backend-side roster agent, a peer session) still gets a row of its own, so
//! nothing that used to be visible disappears. **Sessions** are the rail's own
//! level and are resolved here: a dispatched one by the roster id the hub filed
//! its task under, an operator-started one by [`resolve::agent_for_session`].
//!
//! Row shapes live in [`types`]; the session → agent rule in [`resolve`]; this
//! module is the assembly.

use medulla::config::agent_declarations_for_host;
use medulla::runtime::AgentDeclaration;
use medulla::ui::hosts::{HostAgentRow, HostKind, HostRow};

use super::types::App;
use crate::ui::agents::{ordered_tasks, AgentLane, AgentRole, AgentRow};
use crate::worker::pty::SessionRow;

mod cleanup;
#[cfg(test)]
mod cleanup_tests;
mod cursor;
#[cfg(test)]
mod cursor_tests;
mod organize;
mod paging;
pub(in crate::ui::app) mod resolve;
// Kept apart from `tests` rather than nested inside it: the assembly rules and
// the served-dispatch merge are separate responsibilities, and one file for
// both had already grown past this repository's line ceiling.
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
pub(in crate::ui::app) mod tests;
mod types;

pub(in crate::ui::app) use cursor::rail_anchor;
#[cfg(test)]
pub(in crate::ui::app) use cursor::resolve_rail_cursor;
use types::{AgentGroup, HostGroup};
pub use types::{
    AgentRailRow, GroupRailRow, HostRailRow, RailAnchor, RailRow, SessionRailRow,
    WorkflowRunRailRow,
};

/// The label on the rail's "declare an agent" row.
///
/// It says *agent* rather than *harness* because that is what it produces: a
/// declared `harness × workspace` identity that outlives the session it starts.
pub(in crate::ui::app) const NEW_AGENT_LABEL: &str = "+ New agent";

/// The label on the action row that opens a session under an agent.
///
/// Indented and lower-cased beside [`NEW_AGENT_LABEL`] because it is a leaf of
/// one agent's group rather than an action on the machine.
pub(in crate::ui::app) const NEW_SESSION_LABEL: &str = "+ new session";

impl App {
    /// The agent declarations this machine's config records.
    ///
    /// Read live rather than cached: [`declare_agent`](medulla::config::declare_agent)
    /// writes the file and hands back the list, which is assigned straight into
    /// the loaded config, so the next frame's rail is the list as written.
    pub(in crate::ui::app) fn agent_declarations(&self) -> &[AgentDeclaration] {
        &self.loaded.config.fleet.agent_declarations
    }

    /// The host id this machine's agents are declared against.
    ///
    /// The running host's bus address is the authority — it is what the local
    /// roster stamps every entry with. Without a running host there is nothing
    /// local to place agents on, and the empty string matches the declarations
    /// that name no host.
    pub(in crate::ui::app) fn local_host_id(&self) -> String {
        self.host_obs
            .as_ref()
            .map(|host| host.address().to_string())
            .unwrap_or_default()
    }

    /// The declarations belonging to this machine, in declaration order.
    pub(in crate::ui::app) fn local_agent_declarations(&self) -> Vec<AgentDeclaration> {
        let host_id = self.local_host_id();
        agent_declarations_for_host(self.agent_declarations(), &host_id)
            .into_iter()
            .cloned()
            .collect()
    }

    /// The rail's rows: the conversation, the create action, and the tree.
    ///
    /// Assembled in three passes so each one answers a single question. First the
    /// fold is split — the orchestrator and the function lanes keep their rows,
    /// the agent lanes become groups keyed by agent id. Then the shared
    /// `Host → Agent` projection places those groups, adding every agent that has
    /// no traffic and every host that holds one. Last the live PTY sessions are
    /// attached to whichever agent declares the directory they run in, and the
    /// whole thing is flattened under host rows — which appear only when there is
    /// more than one host to tell apart.
    pub(super) fn rail_rows(&self) -> Vec<RailRow> {
        let lanes = self.lanes();
        self.rail_rows_in(&lanes)
    }

    /// Assemble rail rows from one already-captured lane snapshot.
    ///
    /// Callers that also resolve a cursor anchor must use this with that same
    /// snapshot: lane indexes in fold rows are meaningful only to the lanes
    /// that produced them.
    pub(super) fn rail_rows_in(&self, lanes: &[AgentLane]) -> Vec<RailRow> {
        let (lane_rows, folded) = self.split_fold(lanes);
        let mut hosts = place_agents(&self.host_tree(), folded);
        let mut orphans = self.attach_sessions(&mut hosts);
        let appearance = &self.loaded.config.appearance;
        let sections = organize::organize(
            hosts,
            &self.loaded.config.fleet.agent_declarations,
            appearance.sidebar_grouping,
            appearance.sidebar_sort,
        );
        organize::sort_sessions(&mut orphans, appearance.sidebar_sort);
        self.flatten(lane_rows, sections, orphans, lanes)
    }

    /// Split the folded rows into the non-agent ones and the per-agent groups.
    fn split_fold(&self, lanes: &[AgentLane]) -> (Vec<AgentRow>, Vec<AgentGroup>) {
        let mut lane_rows: Vec<AgentRow> = Vec::new();
        let mut groups: Vec<AgentGroup> = Vec::new();
        for row in self.agent_rows_in(lanes) {
            match row {
                AgentRow::Lane { lane_index } => {
                    let Some(lane) = lanes.get(lane_index) else {
                        continue;
                    };
                    if lane.role != AgentRole::Agent {
                        lane_rows.push(row);
                        continue;
                    }
                    groups.push(self.group_for_lane(lane, lane_index));
                }
                AgentRow::Sub { .. } => {
                    let Some(group) = groups.last_mut() else {
                        continue;
                    };
                    group.visible_tasks += 1;
                }
                AgentRow::More { hidden, .. } => {
                    if let Some(group) = groups.last_mut() {
                        group.hidden += hidden;
                        group.overflow = true;
                    }
                }
                AgentRow::Separator => lane_rows.push(row),
            }
        }
        (lane_rows, groups)
    }

    /// The group an agent-role lane opens.
    ///
    /// The lane's `agent_id` is the roster id the hub filed its tasks under, so
    /// it is also the key the projection's agent is matched by — the two cannot
    /// drift, because the roster is a projection of the declarations. The host id
    /// here is only a hint for a lane the projection turns out not to know; a
    /// placed agent takes its host from the tree.
    fn group_for_lane(&self, lane: &AgentLane, lane_index: usize) -> AgentGroup {
        let agent_id = lane
            .agent_id
            .clone()
            .unwrap_or_else(|| lane.key.trim_start_matches("agent:").to_string());
        let host_id = lane
            .descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.host_id.clone())
            .unwrap_or_default();
        AgentGroup {
            row: AgentRailRow {
                agent_id: agent_id.clone(),
                host_id,
                agent: None,
                lane_index: Some(lane_index),
            },
            sessions: ordered_tasks(&lane.tasks)
                .into_iter()
                .map(|task| SessionRailRow {
                    agent_id: Some(agent_id.clone()),
                    lane_index: Some(lane_index),
                    task: Some(task),
                    local: None,
                    last: false,
                })
                .collect(),
            last_at: lane.last_at,
            lane_label: Some(lane.label.clone()),
            harness_label: lane.harness_label.clone(),
            visible_tasks: 0,
            hidden: 0,
            overflow: false,
        }
    }

    /// Attach the live local sessions to their agents, returning the unclaimed.
    ///
    /// An unclaimed session runs in a directory nothing declares. It is listed at
    /// the end rather than dropped — a session that is running, costing tokens
    /// and invisible is the failure the old `── your harnesses ──` group existed
    /// to prevent — and it is what the inline create-agent offer is for.
    fn attach_sessions(&self, hosts: &mut [HostGroup]) -> Vec<SessionRailRow> {
        let declarations = self.local_agent_declarations();
        let mut groups: Vec<&mut AgentGroup> = hosts
            .iter_mut()
            .flat_map(|host| host.agents.iter_mut())
            .collect();
        let mut orphans = Vec::new();
        for row in self.own_session_rows() {
            // A dispatch this device is serving reaches the rail from both
            // surfaces at once: `split_fold` folded the task from the event
            // stream, and `own_session_rows` lists the live pty so the runs it
            // started have a row to nest under. They are one harness, so the
            // task row takes the local session rather than a second row for the
            // same process appearing beside it — which is exactly the "carries
            // either (or … both)" case [`SessionRailRow`] documents.
            if let Some(existing) = self.task_row_serving(&mut groups, &row.id) {
                existing.local = Some(row);
                continue;
            }
            let agent_id = resolve::agent_for_session(&declarations, &row)
                .map(|declaration| declaration.agent_id.clone());
            let index = agent_id.as_ref().and_then(|agent_id| {
                groups
                    .iter()
                    .position(|group| group.row.agent_id.trim() == agent_id.trim())
            });
            let session = SessionRailRow {
                agent_id,
                lane_index: index.and_then(|index| groups[index].row.lane_index),
                task: None,
                local: Some(row),
                last: false,
            };
            match index {
                Some(index) => groups[index].sessions.push(session),
                None => orphans.push(session),
            }
        }
        orphans
    }

    /// The already-placed task row this device serves with `session_id`, if any.
    ///
    /// Matched through the daemon's running map rather than by name or
    /// directory: `session_for_task` is the same lookup the harness pane
    /// resolves a task's screen with, so the two surfaces cannot disagree about
    /// which pty is serving a dispatch. Rows that already carry a local session
    /// are skipped, so two live sessions never collapse onto one task.
    ///
    /// `None` once the task settles — the runtime drops the record then — which
    /// is the right answer: a retained session outlives its task row and needs a
    /// row of its own.
    fn task_row_serving<'a>(
        &self,
        groups: &'a mut [&mut AgentGroup],
        session_id: &str,
    ) -> Option<&'a mut SessionRailRow> {
        let local_sessions = self.local_sessions.as_ref()?;
        task_row_serving(groups, session_id, |task_id| {
            local_sessions.session_for_task(task_id)
        })
    }

    /// Flatten the tree into rows, wrapping agents in host rows when needed.
    fn flatten(
        &self,
        lane_rows: Vec<AgentRow>,
        sections: Vec<organize::Section>,
        orphans: Vec<SessionRailRow>,
        lanes: &[AgentLane],
    ) -> Vec<RailRow> {
        let mut rows: Vec<RailRow> = lane_rows.into_iter().map(RailRow::Lane).collect();
        // A device that hosts nothing cannot declare an agent on itself, so the
        // action is absent there rather than present and refusing.
        let hosting = self.local_sessions.is_some();
        if hosting {
            rows.push(RailRow::NewAgent);
        }
        // Only over a tree that exists — see [`RailRow::AgentsHeader`]. Counted
        // from the groups rather than from `rows`, because the host rows that
        // wrap them have not been pushed yet.
        if sections.iter().any(|section| !section.agents.is_empty()) {
            rows.push(RailRow::AgentsHeader);
        }
        // Which agents this machine may open a session under: a session is
        // started by the host that owns the agent, so only the agents declared
        // here get the action. Collected once rather than re-scanned per group.
        let declared: Vec<String> = if hosting {
            self.local_agent_declarations()
                .into_iter()
                .map(|declaration| declaration.agent_id)
                .collect()
        } else {
            Vec::new()
        };
        // Which sections exist, in which order, and whether they are headed at
        // all is [`organize`]'s answer, not this one's: it is the operator's
        // Appearance setting, and the assembly here is about what exists.
        for mut section in sections {
            match section.header {
                organize::SectionHeader::Host(host) => rows.push(RailRow::Host(host)),
                organize::SectionHeader::Group(group) => rows.push(RailRow::Group(group)),
                organize::SectionHeader::None => {}
            }
            for group in &mut section.agents {
                let offers_session = declared
                    .iter()
                    .any(|agent_id| agent_id.trim() == group.row.agent_id.trim());
                push_group(
                    &mut rows,
                    group,
                    offers_session,
                    &self.harness_runs,
                    lanes,
                    self.agent_anchor.as_ref(),
                );
            }
        }
        for mut session in orphans {
            session.last = true;
            let session = Box::new(session);
            let runs = run_rows_under(&session, &self.harness_runs);
            rows.push(RailRow::Session(session));
            rows.extend(runs);
        }
        rows
    }

    /// How many local harnesses are waiting on the operator right now.
    ///
    /// Counts every live session on this device, not only the rows on the rail:
    /// a session the orchestrator started and then got stuck on a permission
    /// prompt is exactly the case an operator needs told about, and it may have
    /// no row of its own.
    ///
    /// The attached session is excluded. Its prompt is on screen in front of
    /// the person the count is for, so counting it would ask them to go and
    /// look at what they are already looking at.
    pub(in crate::ui) fn sessions_waiting(&self) -> usize {
        let Some(harnesses) = self.local_sessions.as_ref() else {
            return 0;
        };
        Self::count_waiting(&harnesses.sessions.waiting_sessions(), &self.harness_focus)
    }

    /// The same count from an already-collected waiting set.
    ///
    /// The rail collects that set anyway to style its rows, so the header count
    /// is derived from it instead of taking the lock a second time — and, more
    /// to the point, the header and the rows beneath it are then answering from
    /// one snapshot rather than from two taken a few microseconds apart.
    pub(in crate::ui) fn count_waiting(
        waiting: &std::collections::HashSet<String>,
        focus: &crate::ui::harness_pane::HarnessFocus,
    ) -> usize {
        waiting
            .iter()
            .filter(|id| !focus.is_attached_to(id))
            .count()
    }

    /// The sessions on this device the operator can act on directly.
    ///
    /// A dispatched session reaches the rail through its task while that task is
    /// running, folded from the event stream, so listing it here as well would
    /// show one session twice. What is left is the operator's own — started by
    /// them, or taken from the orchestrator — plus the *retained* ones, which
    /// are dispatched sessions whose task has finished.
    ///
    /// Retained sessions have to be here, and the task row is not a substitute.
    /// A task row carries no local session (`local: None`), so the cursor on one
    /// resolves no pty: the pane cannot draw the live screen and there is
    /// nothing to attach the keyboard to. The task's own screen stops arriving
    /// at the same moment for the same reason — `session_for_task` resolves
    /// through the daemon's *running* map, and the admission guard drops that
    /// record when the task settles. So a finished task's harness is alive and
    /// reachable by nothing until it is listed here, which is the whole point of
    /// having kept it.
    ///
    /// Exited ones do **not** stay listed. A finished session is history, and a
    /// rail that keeps every one of them buries the sessions that are still
    /// doing something under the ones that are not. The two cases where reading
    /// a dead session still matters keep it — the operator is attached to it, or
    /// a run it started is still executing — and [`cleanup`] is where that rule
    /// and the forgetting it drives are written down.
    ///
    /// And a dispatched session that started a workflow run is listed too, task
    /// row or not. The task row carries `local: None`, so it has no grant to key
    /// runs by ([`run_rows_under`]) — which meant the runs an orchestrator's own
    /// harnesses start, the majority of them, were the ones the rail could not
    /// show. A run is minutes-to-hours of work in another process; leaving it
    /// invisible is the same failure retention exists to prevent. That does not
    /// double the row: [`attach_sessions`](Self::attach_sessions) merges such a
    /// session into the task row already standing for it, which is what gives
    /// that row the grant it was missing.
    pub(super) fn own_session_rows(&self) -> Vec<SessionRow> {
        let Some(harnesses) = self.local_sessions.as_ref() else {
            return Vec::new();
        };
        let mut rows: Vec<SessionRow> = harnesses
            .sessions
            .rows()
            .into_iter()
            .filter(|row| {
                row.origin.is_user()
                    || row.control == crate::worker::pty::SessionControl::User
                    || row.retained
                    // Only while a run is *still going*: the rows a settled one
                    // would contribute are dropped by `run_rows_under`, so a
                    // session held open by finished runs would be a row with
                    // nothing under it.
                    || row
                        .mcp_grant_session
                        .as_deref()
                        .is_some_and(|grant| self.harness_runs.any_active_for_session(grant))
            })
            .filter(|row| row.state.is_running() || self.keeps_finished_session(row))
            .collect();
        rows.sort_by_key(|row| row.started_at);
        rows
    }
}

/// Place the folded lanes onto the shared `Host → Agent` tree.
///
/// The tree decides what exists and in what order — it is the same projection
/// the Hosts tab renders, so the two lenses list the same agents under the same
/// hosts. A lane is matched onto its agent by id and contributes only what the
/// projection cannot know: the transcript behind the row, and the tasks folded
/// under it.
///
/// A lane the tree does not know keeps a row of its own. That is not a leftover
/// case: an agent the backend rosters is not necessarily one this hub declares
/// or advertises, and a rail that dropped it would hide work that is running.
fn place_agents(tree: &[HostRow], folded: Vec<AgentGroup>) -> Vec<HostGroup> {
    let mut folded: Vec<Option<AgentGroup>> = folded.into_iter().map(Some).collect();
    let mut hosts: Vec<HostGroup> = tree
        .iter()
        .map(|host| HostGroup {
            row: HostRailRow {
                host_id: host.id.clone(),
                label: host.label.clone(),
                local: host.kind == HostKind::Local,
            },
            agents: host
                .agents
                .iter()
                .map(|agent| placed_agent(agent, &host.id, &mut folded))
                .collect(),
        })
        .collect();
    for group in folded.into_iter().flatten() {
        match unplaced_host(&hosts, &group.row.host_id) {
            Some(index) => hosts[index].agents.push(group),
            None => hosts.push(HostGroup {
                row: HostRailRow {
                    host_id: group.row.host_id.clone(),
                    label: "unplaced".to_string(),
                    local: false,
                },
                agents: vec![group],
            }),
        }
    }
    hosts
}

/// One agent of the tree, with its folded lane taken if it has one.
fn placed_agent(
    agent: &HostAgentRow,
    host_id: &str,
    folded: &mut [Option<AgentGroup>],
) -> AgentGroup {
    let taken = folded
        .iter_mut()
        .find(|group| {
            group
                .as_ref()
                .is_some_and(|group| group.row.agent_id.trim() == agent.agent_id.trim())
        })
        .and_then(Option::take);
    let mut group = taken.unwrap_or_else(|| AgentGroup {
        row: AgentRailRow {
            agent_id: agent.agent_id.clone(),
            host_id: host_id.to_string(),
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
    group.row.host_id = host_id.to_string();
    group.row.agent = Some(agent.clone());
    group
}

/// Where a lane the tree does not know is drawn: the host it names if that host
/// is on the tree, else the machine looking at it.
///
/// `None` only when there is no host at all to hang it from, which is a device
/// that hosts nothing and has declared nothing.
fn unplaced_host(hosts: &[HostGroup], host_id: &str) -> Option<usize> {
    let host_id = host_id.trim();
    hosts
        .iter()
        .position(|host| !host_id.is_empty() && host.row.host_id.trim() == host_id)
        .or_else(|| hosts.iter().position(|host| host.row.local))
}
/// The placed task row `session_id` is serving, given a task-to-session lookup.
///
/// Split from the [`App`] method so the row-picking rule is testable without a
/// daemon: `served` is the only thing the real call needs a running hub for.
fn task_row_serving<'a>(
    groups: &'a mut [&mut AgentGroup],
    session_id: &str,
    served: impl Fn(&str) -> Option<String>,
) -> Option<&'a mut SessionRailRow> {
    groups
        .iter_mut()
        .flat_map(|group| group.sessions.iter_mut())
        .find(|session| {
            session.local.is_none()
                && session
                    .task
                    .as_ref()
                    .is_some_and(|task| served(&task.task_id).as_deref() == Some(session_id))
        })
}

/// The workflow-run rows that belong under one session, oldest first.
///
/// Keyed by the grant session recorded on the PTY row at launch — the same key
/// the MCP subprocess reports under — so a session Medulla did not spawn, or
/// one whose harness was never granted the workflow tools, simply has none.
///
/// Only the runs still executing. A settled run is finished work, and the rail
/// is about work in flight: its record is written to the workflow store the
/// moment it settles, and the Workflows page lists that history properly —
/// with the run's steps, its inputs, and its error — where a rail row could
/// only ever repeat the word "failed" until the session went away.
fn run_rows_under(
    session: &SessionRailRow,
    runs: &medulla::control_socket::HarnessRunRegistry,
) -> Vec<RailRow> {
    let Some(local) = &session.local else {
        return Vec::new();
    };
    let Some(grant) = local.mcp_grant_session.as_deref() else {
        return Vec::new();
    };
    let reported: Vec<_> = runs
        .for_session(grant)
        .into_iter()
        .filter(|run| !run.status.is_terminal())
        .collect();
    let last_index = reported.len().saturating_sub(1);
    reported
        .into_iter()
        .enumerate()
        .map(|(index, run)| {
            RailRow::WorkflowRun(WorkflowRunRailRow {
                session_id: local.id.clone(),
                run,
                // The session's own last-leaf glyph is decided before its runs
                // exist, so the group's real last row is the last run under it.
                last: index == last_index && session.last,
            })
        })
        .collect()
}

/// Delegate paged agent-group rendering to its focused implementation module.
fn push_group(
    rows: &mut Vec<RailRow>,
    group: &mut AgentGroup,
    offers_session: bool,
    runs: &medulla::control_socket::HarnessRunRegistry,
    lanes: &[AgentLane],
    anchor: Option<&RailAnchor>,
) {
    paging::push_group(rows, group, offers_session, runs, lanes, anchor);
}
