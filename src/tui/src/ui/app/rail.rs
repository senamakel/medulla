//! The Agents rail: one cursor over the lanes *and* the declared fleet.
//!
//! The two lists answer adjacent questions — what is running, and what it is
//! running on — and an operator moves between them constantly: an agent stalls,
//! and the next thing you want is the harness it sits on and how much budget
//! that harness has left. Splitting them across tabs meant losing your place in
//! one to look at the other, so they share a rail and a selection here.
//!
//! Rows keep their own models ([`AgentRow`] from the event fold, [`FleetNode`]
//! from the declared capacity); this module only concatenates them, tracks which
//! is selected, and answers what the detail pane should show.

use super::types::App;
use crate::ui::agents::{AgentLane, AgentRole, AgentRow};
use crate::worker::pty::SessionRow;

/// The label on the rail's "start a harness" row.
pub(in crate::ui::app) const NEW_HARNESS_LABEL: &str = "+ New harness";

/// One row of the Agents rail.
#[derive(Debug, Clone)]
pub enum RailRow {
    /// A lane, task sublane, or lane-list divider.
    ///
    /// The lane list's own `── functions ──` separator is an
    /// `AgentRow::Separator`; this variant is the group, not the row type.
    Agent(AgentRow),
    /// The action row that starts a harness of the operator's own.
    ///
    /// Sits directly under the orchestrator lane because that is where the eye
    /// already is — starting a terminal was otherwise a chord (`Ctrl-T`) with
    /// nothing on screen to suggest it exists, which is the same as not having
    /// it for anyone who has not read the bindings.
    NewHarness,
    /// The `── your harnesses ──` divider above the operator's own sessions.
    HarnessSeparator,
    /// A harness the operator started, which no lane will ever describe.
    ///
    /// Lanes are folded from task events, so a session nothing dispatched into
    /// produces none — which is exactly the state an unmanaged harness lives in.
    /// Without its own group it would be running, costing tokens, and invisible.
    Harness(SessionRow),
}

impl RailRow {
    /// Whether the cursor may land on this row.
    pub fn selectable(&self) -> bool {
        match self {
            RailRow::Agent(row) => row.selectable(),
            RailRow::NewHarness => true,
            RailRow::HarnessSeparator => false,
            RailRow::Harness(_) => true,
        }
    }

    /// The PTY session this row names, when it names one directly.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            RailRow::Harness(row) => Some(row.id.as_str()),
            _ => None,
        }
    }

    /// Whether this row is the "start a harness" action.
    pub fn is_new_harness(&self) -> bool {
        matches!(self, RailRow::NewHarness)
    }
}

/// What the rail cursor is *on*, independent of where that row currently sits.
///
/// The rail is rebuilt from scratch every frame out of live state: the fold
/// gains a lane the moment the orchestrator spawns an agent, sublanes reorder as
/// tasks start and finish, and the operator's own harnesses hang below all of
/// it. A cursor stored as a plain row offset therefore points at a *different
/// row* the instant anything above it appears — and for an operator sitting
/// inside an attached harness pane that is not a cosmetic jump: the selection
/// leaves the session, [`App::release_harness`] takes the keyboard back, the
/// composer and work panel reclaim the columns, and the harness is resized and
/// repainted underneath them. It reads exactly like the TUI resetting itself.
///
/// So the cursor is remembered by identity and the offset is re-derived each
/// time the rows are rebuilt. Only rows the cursor can land on have one; the
/// dividers and the `+N more` counter are labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RailAnchor {
    /// The `+ New harness` action row.
    NewHarness,
    /// One of the operator's own harnesses, by PTY session id.
    Harness(String),
    /// A lane header, by [`AgentLane::key`].
    Lane(String),
    /// A task sublane, by owning lane key and task id.
    ///
    /// Keyed on the lane as well as the task because sublanes are only unique
    /// within their lane, and a task row's meaning is "this task, under this
    /// agent".
    Task {
        /// The owning lane's key.
        lane: String,
        /// The task's id.
        task_id: String,
    },
}

/// The identity of `row`, when it is one the cursor can hold.
///
/// `lanes` must be the same lane list `rows` was built from: lane rows carry an
/// index into it, and the key behind that index is what survives the list
/// growing.
pub(in crate::ui::app) fn rail_anchor(row: &RailRow, lanes: &[AgentLane]) -> Option<RailAnchor> {
    match row {
        RailRow::NewHarness => Some(RailAnchor::NewHarness),
        RailRow::Harness(session) => Some(RailAnchor::Harness(session.id.clone())),
        RailRow::HarnessSeparator => None,
        RailRow::Agent(AgentRow::Lane { lane_index }) => lanes
            .get(*lane_index)
            .map(|lane| RailAnchor::Lane(lane.key.clone())),
        RailRow::Agent(AgentRow::Sub {
            lane_index, task, ..
        }) => lanes.get(*lane_index).map(|lane| RailAnchor::Task {
            lane: lane.key.clone(),
            task_id: task.task_id.clone(),
        }),
        // `Separator` and `More` are labels; the cursor steps over them.
        RailRow::Agent(_) => None,
    }
}

/// Where the anchored row sits in `rows` now, or `fallback` when it is gone.
///
/// A row can genuinely disappear — a harness exits and is forgotten, a task
/// scrolls past the sublane cap — and there is no better answer then than the
/// offset the cursor last held, clamped into range. The caller re-anchors from
/// whatever that lands on, so the fallback is used for one frame at most.
pub(in crate::ui::app) fn resolve_rail_cursor(
    rows: &[RailRow],
    lanes: &[AgentLane],
    anchor: Option<&RailAnchor>,
    fallback: usize,
) -> usize {
    if rows.is_empty() {
        return 0;
    }
    if let Some(anchor) = anchor {
        if let Some(index) = rows
            .iter()
            .position(|row| rail_anchor(row, lanes).as_ref() == Some(anchor))
        {
            return index;
        }
    }
    fallback.min(rows.len() - 1)
}

impl App {
    /// The rail's rows: the agent lanes.
    ///
    /// The declared fleet used to hang underneath these, and it was a third
    /// rendering of things that already had two homes. Its agents were the very
    /// lanes above the divider, so a worker that was both connected and declared
    /// appeared twice; its hosts and harnesses are the Routing tab's Harnesses
    /// page, which reads the same `fleet_capacity()`; and its templates were
    /// already excluded here in favour of Routing's Agent Templates page. What
    /// remained was duplication, so the rail now shows what is *running* and
    /// nothing else.
    /// Operator-started harnesses hang below the lanes under their own divider,
    /// because they are the one thing running on this device that the event fold
    /// cannot see. The `+ New harness` action sits between the two, directly
    /// under the orchestrator lane: it is what produces the group below it, and
    /// a device that hosts nothing cannot start one, so it is absent there
    /// rather than present and refusing.
    pub(super) fn rail_rows(&self) -> Vec<RailRow> {
        let lanes = self.lanes();
        let can_start = self.harnesses.is_some();
        let mut rows: Vec<RailRow> = Vec::new();
        let mut placed = !can_start;
        for row in self.agent_rows() {
            let orchestrator = matches!(&row, AgentRow::Lane { lane_index }
                if lanes.get(*lane_index).map(|l| l.role) == Some(AgentRole::Orchestrator));
            rows.push(RailRow::Agent(row));
            if orchestrator && !placed {
                rows.push(RailRow::NewHarness);
                placed = true;
            }
        }
        // No orchestrator lane yet — the very first frame, before any fold has
        // run. The action still belongs on screen, and the top is where the
        // orchestrator will appear above it.
        if !placed {
            rows.insert(0, RailRow::NewHarness);
        }
        let own = self.own_harness_rows();
        if !own.is_empty() {
            rows.push(RailRow::HarnessSeparator);
            rows.extend(own.into_iter().map(RailRow::Harness));
        }
        rows
    }

    /// The rail offset the cursor is on, re-derived from its anchor.
    ///
    /// Every read of the cursor goes through this rather than through
    /// `agent_index` directly, so a rail that grew a row while the operator was
    /// looking elsewhere still answers with the row they picked.
    pub(in crate::ui::app) fn rail_cursor(&self) -> usize {
        self.rail_cursor_in(&self.rail_rows(), &self.lanes())
    }

    /// [`rail_cursor`](Self::rail_cursor) against rows and lanes the caller
    /// already has. Both are derived from the event fold, and rebuilding them
    /// per read costs a full re-fold.
    pub(in crate::ui::app) fn rail_cursor_in(
        &self,
        rows: &[RailRow],
        lanes: &[AgentLane],
    ) -> usize {
        resolve_rail_cursor(rows, lanes, self.agent_anchor.as_ref(), self.agent_index)
    }

    /// Put the cursor on `index`, remembering *which row* that is.
    ///
    /// Every write of the cursor goes through this. Setting `agent_index` alone
    /// leaves the previous anchor in place, and the next frame would drag the
    /// cursor straight back to the old row.
    pub(in crate::ui::app) fn set_rail_cursor(&mut self, index: usize) {
        let rows = self.rail_rows();
        let lanes = self.lanes();
        self.set_rail_cursor_in(&rows, &lanes, index);
    }

    /// [`set_rail_cursor`](Self::set_rail_cursor) against rows and lanes the
    /// caller already has.
    pub(in crate::ui::app) fn set_rail_cursor_in(
        &mut self,
        rows: &[RailRow],
        lanes: &[AgentLane],
        index: usize,
    ) {
        self.agent_index = index.min(rows.len().saturating_sub(1));
        self.agent_anchor = rows
            .get(self.agent_index)
            .and_then(|row| rail_anchor(row, lanes));
    }

    /// Send the cursor back to the top and forget what it was on.
    ///
    /// For the deliberate resets — opening a new thread — where following the
    /// old row would be the wrong behaviour, not the right one.
    pub(in crate::ui::app) fn reset_rail_cursor(&mut self) {
        self.agent_index = 0;
        self.agent_anchor = None;
    }

    /// How many local harnesses are waiting on the operator right now.
    ///
    /// Counts every live session on this device, not only the rows on the rail:
    /// a harness the orchestrator started and then got stuck on a permission
    /// prompt is exactly the case an operator needs told about, and it has no
    /// row of its own — it is somewhere inside a lane.
    ///
    /// The attached session is excluded. Its prompt is on screen in front of
    /// the person the count is for, so counting it would ask them to go and
    /// look at what they are already looking at.
    /// Reads the waiting *ids* rather than [`rows`](crate::worker::pty::PtyManager::rows),
    /// which clones every session's whole row. This runs on the render thread
    /// once per frame for the tab badge, so it takes one lock and copies the
    /// handful of ids that are actually waiting — usually none.
    pub(in crate::ui) fn harnesses_waiting(&self) -> usize {
        let Some(harnesses) = self.harnesses.as_ref() else {
            return 0;
        };
        Self::count_waiting(&harnesses.sessions.waiting_sessions(), &self.harness_focus)
    }

    /// The same count from an already-collected waiting set.
    ///
    /// The rail collects that set anyway to style its lanes, so the header count
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

    /// The harnesses this operator started or now holds, oldest first.
    ///
    /// Exited ones stay listed: the last screen is often the reason it exited,
    /// and a row that vanishes on failure is a row that hides the failure. They
    /// leave when the operator forgets them.
    pub(super) fn own_harness_rows(&self) -> Vec<SessionRow> {
        let Some(harnesses) = self.harnesses.as_ref() else {
            return Vec::new();
        };
        let mut rows: Vec<SessionRow> = harnesses
            .sessions
            .rows()
            .into_iter()
            .filter(|row| {
                row.user_spawned || row.control == crate::worker::pty::HarnessControl::User
            })
            .collect();
        rows.sort_by_key(|row| row.started_at);
        rows
    }
}
